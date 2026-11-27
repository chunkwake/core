package dev.chunkwake.bridge;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.util.ArrayList;
import java.util.Collection;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.nio.file.Path;
import java.util.UUID;
import org.bukkit.Location;
import org.bukkit.NamespacedKey;
import org.bukkit.Particle;
import org.bukkit.World;
import org.bukkit.entity.Entity;
import org.bukkit.entity.LivingEntity;
import org.bukkit.entity.Player;

/**
 * {@link World} — одно измерение сервера, как {@code CraftWorld} над своим
 * {@code ServerLevel}. Все запросы к Rust идут с префиксом {@code @<ключ>}:
 * общее «активное» измерение Rust следует за игроками и для плагина не годится.
 *
 * <p>Имена как у {@code PaperWorldLoader}: {@code world}, {@code world_nether},
 * {@code world_the_end}. UUID и PDC мира — в файлах Paper
 * {@code data/paper/metadata.dat} и {@code data/paper/persistent_data_container.dat}
 * каталога измерения: мир с Paper переносится вместе с ними.
 */
public final class WorldImpl extends org.bukkit.craftbukkit.CraftWorld implements WorldHandle {
    static final String OVERWORLD = "minecraft:overworld";
    static final String NETHER = "minecraft:the_nether";
    static final String END = "minecraft:the_end";

    final World api = new WorldApi(this);
    private final ChunkwakeServer server;
    /** Ключ измерения, например {@code minecraft:the_nether}. */
    final String dim;
    private volatile boolean unloaded;
    private final java.util.Set<Long> loadedChunks = java.util.concurrent.ConcurrentHashMap.newKeySet();
    private volatile UUID uid;
    private volatile PdcImpl pdc;
    private volatile WorldBorderImpl border;

    /** NMS-уровень для плагинов (Orebfuscator: {@code world.getHandle()}). */
    private final net.minecraft.server.level.ServerLevel handle =
            new net.minecraft.server.level.ServerLevel();
    /** Java-зеркало дропнутых предметов для getEntities/getNearbyEntities. */
    private final List<BlockEvents.GroundItem> groundItems =
            new java.util.concurrent.CopyOnWriteArrayList<>();

    WorldImpl(ChunkwakeServer server, String dim) {
        this.server = server;
        this.dim = dim;
        // CE читает поле CraftWorld.world (FieldGetter), а не getHandle();
        // оба должны указывать на один ServerLevel, иначе CE пишет в один
        // мир, а Bukkit/BlockImpl читают из другого.
        this.world = this.handle;
        ((org.bukkit.craftbukkit.CraftWorld) this.api).world = this.handle;
        this.handle.setWorld((org.bukkit.craftbukkit.CraftWorld) this.api);
        this.handle.setDimension(this.dim);
        try {
            Path level = this.server.getLevelDirectory();
            Path store = (level == null ? Path.of(".") : level)
                    .resolve("chunkwake-custom-blocks.txt");
            CustomBlockStore.bind(this.dim, store);
        } catch (Throwable ignored) {
            // без каталога мира persistence просто отключён
        }
    }

    @Override
    public net.minecraft.server.level.ServerLevel getHandle() {
        return this.handle;
    }

    /**
     * Восстанавливает custom-блок из CustomBlockStore через CE place.
     * Вызывается лениво из {@code ServerLevel.getBlockState}.
     */
    public net.minecraft.world.level.block.state.BlockState restoreCustomBlock(
            net.minecraft.core.BlockPos pos) {
        if (CustomBlockStore.restoring()) {
            return null;
        }
        String saved = CustomBlockStore.get(this.dim, pos);
        if (saved == null) {
            return null;
        }
        int bracket = saved.indexOf('[');
        String id = bracket < 0 ? saved : saved.substring(0, bracket);
        String properties = bracket < 0 ? "" : saved.substring(bracket + 1).replace("]", "");
        CustomBlockStore.restoring(true);
        try {
            org.bukkit.plugin.Plugin plugin =
                    org.bukkit.Bukkit.getPluginManager().getPlugin("CraftEngine");
            if (plugin == null) {
                return null;
            }
            ClassLoader loader = plugin.getClass().getClassLoader();
            Class<?> api = Class.forName(
                    "net.momirealms.craftengine.bukkit.api.CraftEngineBlocks", true, loader);
            Class<?> keyClass = Class.forName(
                    "net.momirealms.craftengine.core.util.Key", true, loader);
            Class<?> immutableClass = Class.forName(
                    "net.momirealms.craftengine.core.block.ImmutableBlockState", true, loader);
            Class<?> propertyClass = Class.forName(
                    "net.momirealms.craftengine.core.block.property.Property", true, loader);
            Object key = keyClass.getMethod("of", String.class).invoke(null, id);
            Object definition = api.getMethod("byId", keyClass).invoke(null, key);
            if (definition == null) {
                return null;
            }
            Object state = definition.getClass().getMethod("defaultState").invoke(definition);
            if (!properties.isBlank()) {
                for (String pair : properties.split(",")) {
                    int equals = pair.indexOf('=');
                    if (equals <= 0) {
                        continue;
                    }
                    String name = pair.substring(0, equals).trim();
                    String text = pair.substring(equals + 1).trim();
                    Object property = state.getClass().getMethod("getProperty", String.class)
                            .invoke(state, name);
                    if (property == null) {
                        continue;
                    }
                    Object value = property.getClass().getMethod("valueByName", String.class)
                            .invoke(property, text);
                    if (value == null) {
                        continue;
                    }
                    state = immutableClass.getMethod("with", propertyClass, Comparable.class)
                            .invoke(state, property, value);
                }
            }
            org.bukkit.Location location = new org.bukkit.Location(
                    this.api, pos.getX(), pos.getY(), pos.getZ());
            api.getMethod("place", org.bukkit.Location.class, immutableClass, boolean.class)
                    .invoke(null, location, state, false);
            net.minecraft.world.level.block.state.BlockState overlay = this.handle.overlayState(pos);
            return overlay == null || overlay.isAir() ? null : overlay;
        } catch (Throwable ignored) {
            return null;
        } finally {
            CustomBlockStore.restoring(false);
        }
    }

    /** Запрос к миру этого измерения. */
    String request(String line) {
        return Bridge.world("@" + dim + " " + line);
    }

    /** Запрос с большим ответом (hex-снимок чанка). */
    String requestLarge(String line) {
        byte[] bytes = Bridge.worldBytes("@" + dim + " " + line, 4 << 20);
        return new String(bytes, java.nio.charset.StandardCharsets.UTF_8);
    }

    /** Запрос к миру, которому принадлежит {@code w} (чужой {@link World} — как к оверворлду). */
    static String request(World w, String line) {
        WorldImpl impl = of(w);
        return impl != null ? impl.request(line) : Bridge.world("@" + OVERWORLD + " " + line);
    }

    /** Большой ответ (сундук 54 слота с NBT); вне мира — обычный запрос. */
    static String requestLarge(World w, String line) {
        WorldImpl impl = of(w);
        if (impl != null) {
            return impl.requestLarge(line);
        }
        byte[] bytes = Bridge.worldBytes("@" + OVERWORLD + " " + line, 4 << 20);
        return new String(bytes, java.nio.charset.StandardCharsets.UTF_8);
    }

    public static WorldImpl of(World w) {
        if (w instanceof WorldApi api) {
            return api.impl instanceof WorldImpl wi ? wi : null;
        }
        return Stub.impl(w) instanceof WorldImpl wi ? wi : null;
    }

    public org.bukkit.Chunk[] getLoadedChunks() {
        String data = request("loadedchunks");
        if (data == null || data.isBlank()) {
            return new org.bukkit.Chunk[0];
        }
        java.util.List<org.bukkit.Chunk> chunks = new java.util.ArrayList<>();
        for (String line : data.split("\n")) {
            String[] parts = line.trim().split(" ");
            if (parts.length == 2) {
                try {
                    chunks.add(ChunkImpl.api(this, Integer.parseInt(parts[0]),
                            Integer.parseInt(parts[1])));
                } catch (NumberFormatException ignored) {
                    // пропускаем битую строку
                }
            }
        }
        return chunks.toArray(new org.bukkit.Chunk[0]);
    }

    /** {@code World#isChunkLoaded}: есть ли колонка в активной размерности. */
    public boolean isChunkLoaded(int x, int z) {
        return "1".equals(request("chunkloaded " + x + " " + z).trim());
    }

    public boolean unloadChunk(int x, int z) {
        return unloadChunk(x, z, true);
    }

    public boolean unloadChunk(int x, int z, boolean save) {
        return "1".equals(request("unloadchunk " + x + " " + z
                + (save ? " 1" : " 0")).trim());
    }

    /**
     * {@code World#getEntity(UUID)}: игроки, живые мобы, поддерживаемые
     * немобовые сущности (стрелы, лодки, вагонетки, ТНТ, сферы, облака,
     * глаза Края) и выпавшие предметы этого измерения.
     */
    public org.bukkit.entity.Entity getEntity(UUID uuid) {
        if (uuid == null) {
            return null;
        }
        for (org.bukkit.entity.Entity entity : getEntities()) {
            if (uuid.equals(entity.getUniqueId())) {
                return entity;
            }
        }
        return null;
    }

    /** {@code World#getEntitiesByClasses}: фильтр по классам Bukkit-сущностей. */
    public java.util.Collection<org.bukkit.entity.Entity> getEntitiesByClasses(
            Class<?>... classes) {
        java.util.List<org.bukkit.entity.Entity> out = new java.util.ArrayList<>();
        for (org.bukkit.entity.Entity entity : getEntities()) {
            for (Class<?> type : classes) {
                if (type != null && type.isInstance(entity)) {
                    out.add(entity);
                    break;
                }
            }
        }
        return out;
    }

    private static long chunkKey(int cx, int cz) {
        return ((long) cx << 32) ^ (cz & 0xFFFFFFFFL);
    }

    /** Динамический мир: `getBlockAt` должен подгрузить сохранённый чанк. */
    void ensureChunk(int x, int z) {
        if (dim.equals(OVERWORLD) || dim.equals(NETHER) || dim.equals(END)) {
            return;
        }
        if (loadedChunks.add(chunkKey(x >> 4, z >> 4))) {
            request("loadchunk " + (x >> 4) + " " + (z >> 4));
        }
    }

    public org.bukkit.Chunk getChunkAt(int x, int z) {
        // Bukkit `World#getChunkAt` загружает чанк (а не просто отдаёт дескриптор).
        loadedChunks.add(chunkKey(x, z));
        request("loadchunk " + x + " " + z);
        return ChunkImpl.api(this, x, z);
    }

    public org.bukkit.Chunk getChunkAt(org.bukkit.Location location) {
        return getChunkAt(location.getBlockX() >> 4, location.getBlockZ() >> 4);
    }

    public java.util.concurrent.CompletableFuture<org.bukkit.Chunk> getChunkAtAsync(int x, int z) {
        return java.util.concurrent.CompletableFuture.completedFuture(getChunkAt(x, z));
    }

    public java.util.concurrent.CompletableFuture<org.bukkit.Chunk> getChunkAtAsync(
            int x, int z, boolean generate) {
        return getChunkAtAsync(x, z);
    }

    public java.util.concurrent.CompletableFuture<org.bukkit.Chunk> getChunkAtAsync(
            org.bukkit.Location location) {
        return getChunkAtAsync(location.getBlockX() >> 4, location.getBlockZ() >> 4);
    }

    public java.util.concurrent.CompletableFuture<org.bukkit.Chunk> getChunkAtAsync(
            org.bukkit.Location location, boolean generate) {
        return getChunkAtAsync(location);
    }

    public java.util.concurrent.CompletableFuture<org.bukkit.Chunk> getChunkAtAsync(
            org.bukkit.block.Block block) {
        return getChunkAtAsync(block.getX() >> 4, block.getZ() >> 4);
    }

    public java.util.concurrent.CompletableFuture<org.bukkit.Chunk> getChunkAtAsync(
            org.bukkit.block.Block block, boolean generate) {
        return getChunkAtAsync(block);
    }

    public java.util.concurrent.CompletableFuture<org.bukkit.Chunk> getChunkAtAsyncUrgently(
            int x, int z) {
        return getChunkAtAsync(x, z);
    }

    public java.util.concurrent.CompletableFuture<org.bukkit.Chunk> getChunkAtAsyncUrgently(
            int x, int z, boolean generate) {
        return getChunkAtAsync(x, z);
    }

    public java.util.concurrent.CompletableFuture<org.bukkit.Chunk> getChunkAtAsyncUrgently(
            org.bukkit.Location location) {
        return getChunkAtAsync(location);
    }

    public java.util.concurrent.CompletableFuture<org.bukkit.Chunk> getChunkAtAsyncUrgently(
            org.bukkit.Location location, boolean generate) {
        return getChunkAtAsync(location);
    }

    public java.util.concurrent.CompletableFuture<org.bukkit.Chunk> getChunkAtAsyncUrgently(
            org.bukkit.block.Block block) {
        return getChunkAtAsync(block);
    }

    public java.util.concurrent.CompletableFuture<org.bukkit.Chunk> getChunkAtAsyncUrgently(
            org.bukkit.block.Block block, boolean generate) {
        return getChunkAtAsync(block);
    }

    public void getChunkAtAsync(int x, int z, org.bukkit.World.ChunkLoadCallback callback) {
        if (callback != null) {
            callback.onLoad(getChunkAt(x, z));
        }
    }

    public void getChunkAtAsync(org.bukkit.Location location,
            org.bukkit.World.ChunkLoadCallback callback) {
        if (callback != null) {
            callback.onLoad(getChunkAt(location));
        }
    }

    public void getChunkAtAsync(org.bukkit.block.Block block,
            org.bukkit.World.ChunkLoadCallback callback) {
        if (callback != null) {
            callback.onLoad(getChunkAt(block.getX() >> 4, block.getZ() >> 4));
        }
    }

    public org.bukkit.WorldBorder getWorldBorder() {
        WorldBorderImpl current = this.border;
        if (current == null) {
            synchronized (this) {
                current = this.border;
                if (current == null) {
                    current = new WorldBorderImpl(this.api);
                    this.border = current;
                }
            }
        }
        return current;
    }

    public String getName() {
        if (!dim.equals(OVERWORLD) && !dim.equals(NETHER) && !dim.equals(END)) {            int colon = dim.indexOf(':');
            return colon < 0 ? dim : dim.substring(colon + 1);
        }
        String base = server.levelDirectory().getFileName().toString();
        return switch (dim) {
            case OVERWORLD -> base;
            case NETHER -> base + "_nether";
            case END -> base + "_the_end";
            default -> base + "_" + dim.replace(':', '_');
        };
    }

    void markUnloaded() { this.unloaded = true; }

    boolean isUnloaded() { return this.unloaded; }

    /** Динамический мир (`WorldCreator`), а не оверворлд/Ад/Край. */
    boolean isDynamic() {
        return !dim.equals(OVERWORLD) && !dim.equals(NETHER) && !dim.equals(END);
    }

    public NamespacedKey getKey() { return NamespacedKey.fromString(dim); }
    public net.kyori.adventure.key.Key key() { return getKey(); }

    public World.Environment getEnvironment() {
        return switch (dim) {
            case NETHER -> World.Environment.NETHER;
            case END -> World.Environment.THE_END;
            default -> World.Environment.NORMAL;
        };
    }

    /** `World.getTime`: время суток 0..24000 (не путать с `getFullTime`). */
    public long getTime() {
        String value = request("time").trim();
        return value.isEmpty() ? 0L : Math.floorMod(Long.parseLong(value), 24_000L);
    }

    /** `World.getFullTime`: полное игровое время (game time). */
    public long getFullTime() {
        String value = request("gametime").trim();
        return value.isEmpty() ? 0L : Long.parseLong(value);
    }

    public long getGameTime() { return getFullTime(); }

    public boolean isDayTime() {
        long time = getTime();
        return time < 12_300L || time > 23_850L;
    }

    public void setTime(long time) {
        long normalized = Math.floorMod(time, 24_000L);
        request("settime " + normalized);
        // Bukkit CraftWorld#setTime also advances the full game-time view.
        request("setgametime " + normalized);
    }

    public void setFullTime(long time) {
        request("setgametime " + Math.max(0L, time));
    }

    public long getSeed() {
        // `@<ключ> seed`: у динамического мира (`WorldCreator.seed`) свой сид,
        // у оверворлда/Ада/Края — сид базового уровня.
        String seed = request("seed").trim();
        return seed.isEmpty() ? 0L : Long.parseLong(seed);
    }

    /** Как {@code CraftWorld.getWorldPath}: каталог измерения ({@code dimensions/<ns>/<путь>}). */
    public java.nio.file.Path getWorldPath() {
        NamespacedKey k = getKey();
        return server.levelDirectory().resolve("dimensions").resolve(k.namespace()).resolve(k.value());
    }

    public java.io.File getWorldFolder() { return getWorldPath().toFile(); }

    /** Высоты из {@code dimension_type}: оверворлд −64..320, Ад и Край 0..256. */
    public int getMinHeight() { return dim.equals(OVERWORLD) ? -64 : 0; }
    public int getMaxHeight() { return dim.equals(OVERWORLD) ? 320 : 256; }
    public int getLogicalHeight() { return dim.equals(NETHER) ? 128 : getMaxHeight() - getMinHeight(); }

    /** {@code NoiseGeneratorSettings.seaLevel}: 63, Ад 32, Край 0. */
    public int getSeaLevel() {
        return switch (dim) {
            case NETHER -> 32;
            case END -> 0;
            default -> {
                String value = request("sealevel").trim();
                yield value.isEmpty() ? 63 : Integer.parseInt(value);
            }
        };
    }

    public boolean isNatural() { return dim.equals(OVERWORLD); }
    public boolean isUltraWarm() { return dim.equals(NETHER); }
    public boolean hasSkyLight() { return dim.equals(OVERWORLD); }
    public boolean hasCeiling() { return dim.equals(NETHER); }
    public boolean isPiglinSafe() { return dim.equals(NETHER); }
    public boolean isBedWorks() { return dim.equals(OVERWORLD); }
    public boolean isRespawnAnchorWorks() { return dim.equals(NETHER); }
    public double getCoordinateScale() { return dim.equals(NETHER) ? 8.0 : 1.0; }

    // --- UUID и PDC мира: файлы Paper в `data/paper/` измерения ---

    private java.nio.file.Path paperData(String name) {
        return getWorldPath().resolve("data").resolve("paper").resolve(name + ".dat");
    }

    /** Корень {@code SavedData}: {@code {data: …, DataVersion: n}}. */
    private JsonObject readSaved(String name) {
        if (Bridge.table == null) {
            return null;
        }
        String json = Bridge.world("nbtread " + paperData(name));
        if (json.isEmpty()) {
            return null;
        }
        JsonObject root = JsonParser.parseString(json).getAsJsonObject();
        return root.has("data") && root.getAsJsonObject("data").has("c")
                ? root.getAsJsonObject("data").getAsJsonObject("c") : null;
    }

    private void writeSaved(String name, JsonObject data) {
        if (Bridge.table == null) {
            return;
        }
        JsonObject root = new JsonObject();
        JsonObject wrapped = new JsonObject();
        wrapped.add("c", data);
        root.add("data", wrapped);
        JsonObject version = new JsonObject();
        version.addProperty("i", ChunkwakeServer.Unsafe.DATA_VERSION);
        root.add("DataVersion", version);
        Bridge.world("nbtwrite " + paperData(name) + "\n" + root);
    }

    /**
     * {@code PaperWorldMetadata.uuid}: из файла, а нет файла — случайный и
     * сразу на диск, как {@code PaperWorldLoader.loadWorldData}.
     */
    public UUID getUID() {
        UUID u = uid;
        if (u != null) {
            return u;
        }
        synchronized (this) {
            if (uid != null) {
                return uid;
            }
            JsonObject data = readSaved("metadata");
            if (data != null && data.has("uuid") && data.getAsJsonObject("uuid").has("ia")) {
                var a = data.getAsJsonObject("uuid").getAsJsonArray("ia");
                long most = (long) a.get(0).getAsInt() << 32 | a.get(1).getAsInt() & 0xFFFFFFFFL;
                long least = (long) a.get(2).getAsInt() << 32 | a.get(3).getAsInt() & 0xFFFFFFFFL;
                uid = new UUID(most, least);
            } else {
                boolean custom = !dim.equals(OVERWORLD) && !dim.equals(NETHER) && !dim.equals(END);
                uid = (custom || Bridge.table == null)
                        ? UUID.nameUUIDFromBytes(dim.getBytes(java.nio.charset.StandardCharsets.UTF_8))
                        : UUID.randomUUID();
                JsonObject out = new JsonObject();
                JsonObject ia = new JsonObject();
                var arr = new com.google.gson.JsonArray();
                arr.add((int) (uid.getMostSignificantBits() >> 32));
                arr.add((int) uid.getMostSignificantBits());
                arr.add((int) (uid.getLeastSignificantBits() >> 32));
                arr.add((int) uid.getLeastSignificantBits());
                ia.add("ia", arr);
                out.add("uuid", ia);
                writeSaved("metadata", out);
            }
            return uid;
        }
    }

    /** {@code PaperWorldPDC}: правка сразу пишется в файл мира. */
    public org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer() {
        PdcImpl p = pdc;
        if (p == null) {
            synchronized (this) {
                if (pdc == null) {
                    JsonObject data = readSaved("persistent_data_container");
                    Map<String, Object> tags = data == null ? new LinkedHashMap<>() : PdcImpl.Nbt.decode(data);
                    pdc = new PdcImpl(tags, t -> writeSaved("persistent_data_container", PdcImpl.Nbt.encode(t)));
                }
                p = pdc;
            }
        }
        return p.api;
    }

    // --- Метаданные (`Metadatable`): `WorldMetadataStore` по UUID мира ---

    public void setMetadata(String key, org.bukkit.metadata.MetadataValue value) {
        server.metadata().worlds.setMetadata(api, key, value);
    }
    public List<org.bukkit.metadata.MetadataValue> getMetadata(String key) {
        return server.metadata().worlds.getMetadata(api, key);
    }
    public boolean hasMetadata(String key) { return server.metadata().worlds.hasMetadata(api, key); }
    public void removeMetadata(String key, org.bukkit.plugin.Plugin owner) {
        server.metadata().worlds.removeMetadata(api, key, owner);
    }

    // --- Геймрулы: те же правила, что у `/gamerule` (общие на сервер) ---

    public <T> T getGameRuleValue(org.bukkit.GameRule<T> rule) {
        return rule instanceof RegistryValues.GameRuleImpl<?> r ? rule.getType().cast(
                RegistryValues.GameRules.value(r)) : null;
    }

    public <T> T getGameRuleDefault(org.bukkit.GameRule<T> rule) {
        return rule == null ? null : rule.getDefaultValue();
    }

    public <T> boolean setGameRule(org.bukkit.GameRule<T> rule, T value) {
        return rule instanceof RegistryValues.GameRuleImpl<?> r && value != null
                && RegistryValues.GameRules.set(r, value);
    }

    /** Имена как у {@code GameRule.getName()}: {@code minecraft:keep_inventory}. */
    public String[] getGameRules() {
        return RegistryValues.GameRules.names().stream().map(n -> "minecraft:" + n).toArray(String[]::new);
    }

    public boolean isGameRule(String name) {
        return rule(name) != null;
    }

    public String getGameRuleValue(String name) {
        org.bukkit.GameRule<?> r = rule(name);
        Object v = r == null ? null : getGameRuleValue(r);
        return v == null ? null : v.toString();
    }

    @SuppressWarnings("unchecked")
    public boolean setGameRuleValue(String name, String value) {
        org.bukkit.GameRule<Object> r = (org.bukkit.GameRule<Object>) rule(name);
        if (r == null || value == null) {
            return false;
        }
        Object parsed;
        if ((Class<?>) r.getType() == Integer.class) {
            try {
                parsed = Integer.parseInt(value);
            } catch (NumberFormatException e) {
                return false;
            }
        } else if (value.equals("true") || value.equals("false")) {
            parsed = Boolean.parseBoolean(value);
        } else {
            return false;
        }
        return setGameRule(r, parsed);
    }

    private static org.bukkit.GameRule<?> rule(String name) {
        NamespacedKey k = name == null ? null : NamespacedKey.fromString(name);
        return k == null ? null : org.bukkit.Registry.GAME_RULE.get(k);
    }

    // --- Блоки ---

    public Location getSpawnLocation() {
        String[] p = request("spawnpos").trim().split(" ");
        if (p.length == 3) {
            try {
                return new Location(api, Double.parseDouble(p[0]), Double.parseDouble(p[1]),
                        Double.parseDouble(p[2]));
            } catch (NumberFormatException ignored) {
                // ниже — запасная точка, как в старом поведении
            }
        }
        return new Location(api, 0, 64, 0);
    }

    public boolean setSpawnLocation(int x, int y, int z) {
        request("spawnpos " + x + " " + y + " " + z);
        return true;
    }

    public boolean setSpawnLocation(Location location) {
        return setSpawnLocation(location.getBlockX(), location.getBlockY(), location.getBlockZ());
    }

    /** {@code World#getPVP}: серверный флаг из {@code server.properties}. */
    public boolean getPVP() {
        return "1".equals(request("pvp").trim());
    }

    public void setPVP(boolean pvp) {
        request("pvp " + (pvp ? "1" : "0"));
    }

    /** {@code World#getDifficulty}: сложность сервера (общая для всех миров). */
    public org.bukkit.Difficulty getDifficulty() {
        try {
            return org.bukkit.Difficulty.getByValue(Integer.parseInt(request("difficulty").trim()));
        } catch (NumberFormatException e) {
            return org.bukkit.Difficulty.NORMAL;
        }
    }

    public void setDifficulty(org.bukkit.Difficulty difficulty) {
        if (difficulty != null) {
            request("difficulty " + difficulty.getValue());
        }
    }

    /** {@code World#isClearWeather}: сейчас нет дождя и грозы. */
    public boolean isClearWeather() {
        return "1".equals(request("clearweather").trim());
    }
    public org.bukkit.block.Block getBlockAt(int x, int y, int z) {
        return new BlockImpl(api, x, y, z, null).api;
    }
    public org.bukkit.block.Block getBlockAt(Location l) {
        return getBlockAt(l.getBlockX(), l.getBlockY(), l.getBlockZ());
    }
    public org.bukkit.Material getType(int x, int y, int z) { return getBlockAt(x, y, z).getType(); }
    public void setType(int x, int y, int z, org.bukkit.Material m) { getBlockAt(x, y, z).setType(m); }

    /**
     * `World.save()`: просит сервер записать мир на диск.
     *
     * BlueMap зовёт его при включении, чтобы свойства мира точно были в
     * файле. Запись идёт в блокирующем пуле сервера (как автосейв), здесь
     * только просьба: иначе плагин ждал бы диск на своём потоке.
     */
    public void save() {
        Bridge.world("save");
    }

    public void save(boolean force) {
        save();
    }

    /** Автосохранение у нас всегда включено: раз в 30 с и по просьбе. */
    public boolean isAutoSave() { return true; }

    public void setAutoSave(boolean value) {
        // Выключить автосейв плагину нельзя: мир потерял бы данные при
        // аварии. Значение принимается молча, как в ваниле без правок.
    }

    // --- Сущности ---

    public Entity spawnEntity(Location l, org.bukkit.entity.EntityType type) {
        String s = MobImpl.species(type);
        if (s != null) {
            return MobImpl.spawn(this, l, s);
        }
        if (NonMobImpl.supported(type)) {
            return NonMobImpl.spawn(this, l, type);
        }
        throw new UnsupportedOperationException("chunkwake: spawnEntity " + type);
    }

    public Entity spawnEntity(Location l, org.bukkit.entity.EntityType type, boolean randomize) {
        return spawnEntity(l, type);
    }

    public Entity spawnEntity(Location l, org.bukkit.entity.EntityType type,
            org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason reason) {
        return spawnEntity(l, type);
    }

    public Entity spawnEntity(Location l, org.bukkit.entity.EntityType type,
            org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason reason,
            java.util.function.Consumer<? super Entity> init) {
        Entity e = spawnEntity(l, type);
        if (init != null) {
            init.accept(e);
        }
        return e;
    }

    public <T extends Entity> T spawn(Location l, Class<T> type) {
        return spawn(l, type, (java.util.function.Consumer<? super T>) null);
    }

    public <T extends Entity> T spawn(Location l, Class<T> type, org.bukkit.util.Consumer<? super T> init) {
        java.util.function.Consumer<? super T> c = init == null ? null : init::accept;
        return spawn(l, type, c);
    }

    public <T extends Entity> T spawn(Location l, Class<T> type,
            org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason reason) {
        return spawn(l, type, (java.util.function.Consumer<? super T>) null);
    }

    public <T extends Entity> T spawn(Location l, Class<T> type,
            org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason reason,
            java.util.function.Consumer<? super T> init) {
        return spawn(l, type, init);
    }

    public <T extends Entity> T spawn(Location l, Class<T> type,
            java.util.function.Consumer<? super T> init,
            org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason reason) {
        return spawn(l, type, init);
    }

    public <T extends Entity> T spawn(Location l, Class<T> type, boolean randomize,
            java.util.function.Consumer<? super T> init) {
        return spawn(l, type, init);
    }

    public <T extends Entity> T spawn(Location l, Class<T> type, java.util.function.Consumer<? super T> init) {
        Entity e;
        String s = MobImpl.species(type);
        if (s != null) {
            e = MobImpl.spawn(this, l, s);
        } else {
            org.bukkit.entity.EntityType et = NonMobImpl.type(type);
            if (et == null) {
                throw new UnsupportedOperationException("chunkwake: spawn " + type.getSimpleName());
            }
            e = NonMobImpl.spawn(this, l, et);
        }
        T result = type.cast(e);
        if (init != null) {
            init.accept(result);
        }
        return result;
    }

    /** Игроки этого мира, как {@code CraftWorld.getPlayers}. */
    public List<Player> getPlayers() {
        List<Player> out = new ArrayList<>();
        for (Player p : server.getOnlinePlayers()) {
            if (p.getWorld() == api) {
                out.add(p);
            }
        }
        return out;
    }

    public int getPlayerCount() { return getPlayers().size(); }

    // --- звуки мира: Bukkit/Paper World.playSound/stopSound ---

    /** Ванильный радиус позиционного звука: 16 блоков, при volume > 1 — 16*volume. */
    private static double soundRange(float volume) {
        return volume > 1.0F ? 16.0D * volume : 16.0D;
    }

    private void playWorldSound(Object sound, Object source, double x, double y, double z,
            float volume, float pitch) {
        if (sound == null || source == null) {
            return;
        }
        double range = soundRange(volume);
        double rangeSq = range * range;
        for (Player p : getPlayers()) {
            Location at = p.getLocation();
            double dx = at.getX() - x;
            double dy = at.getY() - y;
            double dz = at.getZ() - z;
            PlayerImpl impl = server.playerImpl(p);
            if (impl != null && dx * dx + dy * dy + dz * dz <= rangeSq) {
                impl.worldSound(sound, source, x, y, z, volume, pitch);
            }
        }
    }

    /**
     * Звук сущности: Paper шлёт его трекерам сущности; в Java-мосте трекеров нет,
     * поэтому берём 48 блоков (радиус большинства сущностей) либо больше при volume > 1.
     */
    private void playEntitySound(Entity entity, Object sound, Object source,
            float volume, float pitch, long seed) {
        if (entity == null || sound == null || source == null || entity.getWorld() != api) {
            return;
        }
        Location at = entity.getLocation();
        double range = soundRange(volume);
        double rangeSq = range * range;
        for (Player p : getPlayers()) {
            PlayerImpl impl = server.playerImpl(p);
            if (impl != null && p.getLocation().distanceSquared(at) <= rangeSq) {
                impl.entitySound(entity.getEntityId(), sound, source, volume, pitch, seed);
            }
        }
    }

    public void playSound(Location loc, org.bukkit.Sound sound, float volume, float pitch) {
        playSound(loc, sound, org.bukkit.SoundCategory.MASTER, volume, pitch);
    }

    public void playSound(Location loc, String sound, float volume, float pitch) {
        playSound(loc, sound, org.bukkit.SoundCategory.MASTER, volume, pitch);
    }

    public void playSound(Location loc, org.bukkit.Sound sound,
            org.bukkit.SoundCategory category, float volume, float pitch) {
        playSound(loc, sound, category, volume, pitch, 0L);
    }

    public void playSound(Location loc, String sound,
            org.bukkit.SoundCategory category, float volume, float pitch) {
        playSound(loc, sound, category, volume, pitch, 0L);
    }

    public void playSound(Location loc, org.bukkit.Sound sound,
            org.bukkit.SoundCategory category, float volume, float pitch, long seed) {
        if (loc != null) {
            playWorldSound(sound, category, loc.getX(), loc.getY(), loc.getZ(), volume, pitch);
        }
    }

    public void playSound(Location loc, String sound,
            org.bukkit.SoundCategory category, float volume, float pitch, long seed) {
        if (loc != null) {
            playWorldSound(sound, category, loc.getX(), loc.getY(), loc.getZ(), volume, pitch);
        }
    }

    public void playSound(Entity entity, org.bukkit.Sound sound, float volume, float pitch) {
        playSound(entity, sound, org.bukkit.SoundCategory.MASTER, volume, pitch);
    }

    public void playSound(Entity entity, String sound, float volume, float pitch) {
        playSound(entity, sound, org.bukkit.SoundCategory.MASTER, volume, pitch);
    }

    public void playSound(Entity entity, org.bukkit.Sound sound,
            org.bukkit.SoundCategory category, float volume, float pitch) {
        playSound(entity, sound, category, volume, pitch, 0L);
    }

    public void playSound(Entity entity, String sound,
            org.bukkit.SoundCategory category, float volume, float pitch) {
        playSound(entity, sound, category, volume, pitch, 0L);
    }

    public void playSound(Entity entity, org.bukkit.Sound sound,
            org.bukkit.SoundCategory category, float volume, float pitch, long seed) {
        playEntitySound(entity, sound, category, volume, pitch, seed);
    }

    public void playSound(Entity entity, String sound,
            org.bukkit.SoundCategory category, float volume, float pitch, long seed) {
        playEntitySound(entity, sound, category, volume, pitch, seed);
    }

    public void playSound(net.kyori.adventure.sound.Sound sound) {
        if (sound == null) {
            return;
        }
        for (Player p : getPlayers()) {
            PlayerImpl impl = server.playerImpl(p);
            if (impl != null) {
                Location at = p.getLocation();
                impl.worldSound(sound, sound.source(), at.getX(), at.getY(), at.getZ(),
                        sound.volume(), sound.pitch());
            }
        }
    }

    public void playSound(net.kyori.adventure.sound.Sound sound, double x, double y, double z) {
        if (sound == null) {
            return;
        }
        playWorldSound(sound, sound.source(), x, y, z, sound.volume(), sound.pitch());
    }

    public void playSound(net.kyori.adventure.sound.Sound sound,
            io.papermc.paper.math.Position position) {
        if (sound != null && position != null) {
            playSound(sound, position.x(), position.y(), position.z());
        }
    }

    public void playSound(net.kyori.adventure.sound.Sound sound,
            net.kyori.adventure.sound.Sound.Emitter emitter) {
        if (sound == null) {
            return;
        }
        if (emitter == net.kyori.adventure.sound.Sound.Emitter.self()) {
            playSound(sound);
            return;
        }
        if (emitter instanceof Entity entity) {
            playEntitySound(entity, sound, sound.source(), sound.volume(), sound.pitch(), 0L);
            return;
        }
        throw new IllegalArgumentException(
                "Sound emitter must be an Entity or self(), but was: " + emitter);
    }

    public void stopSound(net.kyori.adventure.sound.SoundStop stop) {
        if (stop == null) {
            return;
        }
        for (Player p : getPlayers()) {
            p.stopSound(stop);
        }
    }

    public void stopSound(net.kyori.adventure.sound.Sound sound) {
        if (sound == null) {
            return;
        }
        for (Player p : getPlayers()) {
            p.stopSound(sound);
        }
    }

    // --- частицы: World.spawnParticle ---

    /** Общий путь мировых частиц: Rust сам фильтрует зрителей по измерению и радиусу. */
    private void spawnParticleWorld(Particle particle, double x, double y, double z, int count,
            double offsetX, double offsetY, double offsetZ, double speed,
            Object data, boolean force) {
        String line = ParticleWire.command(particle, x, y, z, count,
                offsetX, offsetY, offsetZ, speed, data, force);
        if (line != null) {
            request("fx " + line);
        }
    }

    public void spawnParticle(Particle particle, Location location, int count) {
        if (location == null) {
            throw new IllegalArgumentException("Location cannot be null");
        }
        spawnParticleWorld(particle, location.getX(), location.getY(), location.getZ(),
                count, 0.0, 0.0, 0.0, 0.0, null, false);
    }

    public void spawnParticle(Particle particle, double x, double y, double z, int count) {
        spawnParticleWorld(particle, x, y, z, count, 0.0, 0.0, 0.0, 0.0, null, false);
    }

    public <T> void spawnParticle(Particle particle, Location location, int count, T data) {
        if (location == null) {
            throw new IllegalArgumentException("Location cannot be null");
        }
        spawnParticleWorld(particle, location.getX(), location.getY(), location.getZ(),
                count, 0.0, 0.0, 0.0, 0.0, data, false);
    }

    public <T> void spawnParticle(Particle particle, double x, double y, double z, int count,
            T data) {
        spawnParticleWorld(particle, x, y, z, count, 0.0, 0.0, 0.0, 0.0, data, false);
    }

    public void spawnParticle(Particle particle, Location location, int count,
            double offsetX, double offsetY, double offsetZ) {
        if (location == null) {
            throw new IllegalArgumentException("Location cannot be null");
        }
        spawnParticleWorld(particle, location.getX(), location.getY(), location.getZ(),
                count, offsetX, offsetY, offsetZ, 0.0, null, false);
    }

    public void spawnParticle(Particle particle, double x, double y, double z, int count,
            double offsetX, double offsetY, double offsetZ) {
        spawnParticleWorld(particle, x, y, z, count,
                offsetX, offsetY, offsetZ, 0.0, null, false);
    }

    public <T> void spawnParticle(Particle particle, Location location, int count,
            double offsetX, double offsetY, double offsetZ, T data) {
        if (location == null) {
            throw new IllegalArgumentException("Location cannot be null");
        }
        spawnParticleWorld(particle, location.getX(), location.getY(), location.getZ(),
                count, offsetX, offsetY, offsetZ, 0.0, data, false);
    }

    public <T> void spawnParticle(Particle particle, double x, double y, double z, int count,
            double offsetX, double offsetY, double offsetZ, T data) {
        spawnParticleWorld(particle, x, y, z, count,
                offsetX, offsetY, offsetZ, 0.0, data, false);
    }

    public void spawnParticle(Particle particle, Location location, int count,
            double offsetX, double offsetY, double offsetZ, double speed) {
        if (location == null) {
            throw new IllegalArgumentException("Location cannot be null");
        }
        spawnParticleWorld(particle, location.getX(), location.getY(), location.getZ(),
                count, offsetX, offsetY, offsetZ, speed, null, false);
    }

    public void spawnParticle(Particle particle, double x, double y, double z, int count,
            double offsetX, double offsetY, double offsetZ, double speed) {
        spawnParticleWorld(particle, x, y, z, count,
                offsetX, offsetY, offsetZ, speed, null, false);
    }

    public <T> void spawnParticle(Particle particle, Location location, int count,
            double offsetX, double offsetY, double offsetZ, double speed, T data) {
        if (location == null) {
            throw new IllegalArgumentException("Location cannot be null");
        }
        spawnParticleWorld(particle, location.getX(), location.getY(), location.getZ(),
                count, offsetX, offsetY, offsetZ, speed, data, false);
    }

    public <T> void spawnParticle(Particle particle, double x, double y, double z, int count,
            double offsetX, double offsetY, double offsetZ, double speed, T data) {
        spawnParticleWorld(particle, x, y, z, count,
                offsetX, offsetY, offsetZ, speed, data, false);
    }

    public <T> void spawnParticle(Particle particle, Location location, int count,
            double offsetX, double offsetY, double offsetZ, double speed, T data, boolean force) {
        if (location == null) {
            throw new IllegalArgumentException("Location cannot be null");
        }
        spawnParticleWorld(particle, location.getX(), location.getY(), location.getZ(),
                count, offsetX, offsetY, offsetZ, speed, data, force);
    }

    public <T> void spawnParticle(Particle particle, double x, double y, double z, int count,
            double offsetX, double offsetY, double offsetZ, double speed, T data, boolean force) {
        spawnParticleWorld(particle, x, y, z, count,
                offsetX, offsetY, offsetZ, speed, data, force);
    }

    /**
     * Ядро Paper с явным списком получателей. Список {@code null} — все игроки
     * мира; {@code source} в мосте не влияет на видимость (трекеров нет).
     * {@code randomizationType} пока не передаётся в пакет (Rust шлёт DEFAULT).
     */
    public <T> void spawnParticle(Particle particle, List<Player> receivers, Player source,
            double x, double y, double z, int count,
            double offsetX, double offsetY, double offsetZ,
            double speedX, double speedY, double speedZ,
            T data, boolean force, Particle.RandomizationType randomizationType) {
        String line = ParticleWire.command(particle, x, y, z, count,
                offsetX, offsetY, offsetZ, speedX, data, force);
        if (line == null) {
            return;
        }
        List<Player> viewers = receivers != null ? receivers : getPlayers();
        for (Player viewer : viewers) {
            if (viewer == null) {
                continue;
            }
            PlayerImpl impl = server.playerImpl(viewer);
            if (impl != null) {
                impl.sendParticleCommand(line);
            }
        }
    }

    // --- мировые эффекты: молния, взрыв, стрелы ---

    public org.bukkit.entity.LightningStrike strikeLightning(Location loc) {
        return strikeLightning0(loc, false);
    }

    public org.bukkit.entity.LightningStrike strikeLightningEffect(Location loc) {
        return strikeLightning0(loc, true);
    }

    private org.bukkit.entity.LightningStrike strikeLightning0(Location loc, boolean effect) {
        if (loc == null) {
            throw new IllegalArgumentException("Location cannot be null");
        }
        LightningStrikeImpl bolt = new LightningStrikeImpl(this, loc, effect);
        var event = new org.bukkit.event.weather.LightningStrikeEvent(
                api, (org.bukkit.entity.LightningStrike) bolt.api,
                org.bukkit.event.weather.LightningStrikeEvent.Cause.CUSTOM);
        server.api.getPluginManager().callEvent(event);
        if (event.isCancelled()) {
            return (org.bukkit.entity.LightningStrike) bolt.api;
        }
        String reply = request("fx lightning " + loc.getX() + " " + loc.getY() + " " + loc.getZ()
                + " " + (effect ? 1 : 0)).trim();
        if (!reply.isEmpty()) {
            try {
                bolt.id = Integer.parseInt(reply);
            } catch (NumberFormatException ignored) {
                // id нужен только для getEntityId; сам эффект уже создан
            }
        }
        return (org.bukkit.entity.LightningStrike) bolt.api;
    }

    /** Общая точка входа {@code createExplosion}; Bukkit-события взрыва пока не поднимаются. */
    private boolean explode(Location loc, float power, boolean setFire, boolean breakBlocks) {
        if (loc == null) {
            throw new IllegalArgumentException("Location cannot be null");
        }
        request("fx explosion " + loc.getX() + " " + loc.getY() + " " + loc.getZ()
                + " " + power + " " + (setFire ? 1 : 0) + " " + (breakBlocks ? 1 : 0));
        return true;
    }

    public boolean createExplosion(Location loc, float power) {
        return explode(loc, power, false, true);
    }

    public boolean createExplosion(Location loc, float power, boolean setFire) {
        return explode(loc, power, setFire, true);
    }

    public boolean createExplosion(Location loc, float power, boolean setFire, boolean breakBlocks) {
        return explode(loc, power, setFire, breakBlocks);
    }

    public boolean createExplosion(Location loc, float power, boolean setFire, boolean breakBlocks,
            Entity source) {
        return explode(loc, power, setFire, breakBlocks);
    }

    public boolean createExplosion(Entity source, Location loc, float power) {
        return explode(loc, power, false, true);
    }

    public boolean createExplosion(Entity source, Location loc, float power, boolean setFire) {
        return explode(loc, power, setFire, true);
    }

    public boolean createExplosion(Entity source, Location loc, float power, boolean setFire,
            boolean breakBlocks) {
        return explode(loc, power, setFire, breakBlocks);
    }

    public boolean createExplosion(Entity source, Location loc, float power, boolean setFire,
            boolean breakBlocks, boolean excludeSource) {
        return explode(loc, power, setFire, breakBlocks);
    }

    public boolean createExplosion(double x, double y, double z, float power) {
        return explode(new Location(api, x, y, z), power, false, true);
    }

    public boolean createExplosion(double x, double y, double z, float power, boolean setFire) {
        return explode(new Location(api, x, y, z), power, setFire, true);
    }

    public boolean createExplosion(double x, double y, double z, float power, boolean setFire,
            boolean breakBlocks) {
        return explode(new Location(api, x, y, z), power, setFire, breakBlocks);
    }

    public boolean createExplosion(double x, double y, double z, float power, boolean setFire,
            boolean breakBlocks, Entity source) {
        return explode(new Location(api, x, y, z), power, setFire, breakBlocks);
    }

    public boolean createExplosion(Entity source, float power) {
        return source != null && explode(source.getLocation(), power, false, true);
    }

    public boolean createExplosion(Entity source, float power, boolean setFire) {
        return source != null && explode(source.getLocation(), power, setFire, true);
    }

    public boolean createExplosion(Entity source, float power, boolean setFire, boolean breakBlocks) {
        return source != null && explode(source.getLocation(), power, setFire, breakBlocks);
    }

    public org.bukkit.entity.Arrow spawnArrow(Location loc, org.bukkit.util.Vector direction,
            float speed, float spread) {
        return spawnArrow(loc, direction, speed, spread, org.bukkit.entity.Arrow.class);
    }

    public <T extends org.bukkit.entity.AbstractArrow> T spawnArrow(Location loc,
            org.bukkit.util.Vector direction, float speed, float spread, Class<T> clazz) {
        return ArrowSpawn.spawn(this, loc, direction, speed, spread, clazz);
    }

    public List<Entity> getEntities() {
        List<Entity> out = new ArrayList<>(getPlayers());
        out.addAll(MobImpl.all(this));
        out.addAll(NonMobImpl.all(this));
        for (BlockEvents.GroundItem item : this.groundItems) {
            out.add(Stub.of(org.bukkit.entity.Item.class, item));
        }
        return out;
    }

    public List<LivingEntity> getLivingEntities() {
        List<LivingEntity> out = new ArrayList<>(getPlayers());
        out.addAll(MobImpl.all(this));
        return out;
    }

    public <T extends Entity> Collection<T> getEntitiesByClass(Class<T> type) {
        List<T> out = new ArrayList<>();
        for (var e : getEntities()) {
            if (type.isInstance(e)) {
                out.add(type.cast(e));
            }
        }
        return out;
    }

    public Collection<Entity> getNearbyEntities(Location l, double x, double y, double z) {
        List<Entity> out = new ArrayList<>();
        for (var e : getEntities()) {
            var at = e.getLocation();
            if (Math.abs(at.getX() - l.getX()) <= x && Math.abs(at.getY() - l.getY()) <= y
                    && Math.abs(at.getZ() - l.getZ()) <= z) {
                out.add(e);
            }
        }
        return out;
    }

    /** Предмет на земле без разброса. */
    public org.bukkit.entity.Item dropItem(Location l, org.bukkit.inventory.ItemStack s) {
        return drop(l, s, false);
    }

    /** Предмет с разбросом, как из сломанного блока. */
    public org.bukkit.entity.Item dropItemNaturally(Location l, org.bukkit.inventory.ItemStack s) {
        return drop(l, s, true);
    }

    private org.bukkit.entity.Item drop(Location l, org.bukkit.inventory.ItemStack s, boolean natural) {
        String line = StackImpl.line(s);
        if (line == null) {
            throw new IllegalArgumentException("Cannot drop air");
        }
        request("dropitem " + l.getX() + " " + l.getY() + " " + l.getZ() + " " + (natural ? 1 : 0) + " " + line);
        BlockEvents.GroundItem item = new BlockEvents.GroundItem(l.clone(), s.clone());
        this.groundItems.add(item);
        // Не даём Java-зеркалу расти бесконечно на долгоживущем мире.
        while (this.groundItems.size() > 4096) {
            this.groundItems.remove(0);
        }
        return Stub.of(org.bukkit.entity.Item.class, item);
    }

    @Override
    public String toString() {
        return "CraftWorld{name=" + getName() + "}";
    }
}
