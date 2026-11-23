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

    /** Запрос к миру, которому принадлежит {@code w} (чужой {@link World} — как к оверворлду). */
    static String request(World w, String line) {
        WorldImpl impl = of(w);
        return impl != null ? impl.request(line) : Bridge.world("@" + OVERWORLD + " " + line);
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

    public org.bukkit.Chunk getChunkAt(int x, int z) {
        return ChunkImpl.api(this, x, z);
    }

    public org.bukkit.Chunk getChunkAt(org.bukkit.Location location) {
        return getChunkAt(location.getBlockX() >> 4, location.getBlockZ() >> 4);
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
        String base = server.levelDirectory().getFileName().toString();
        return switch (dim) {
            case OVERWORLD -> base;
            case NETHER -> base + "_nether";
            case END -> base + "_the_end";
            default -> base + "_" + dim.replace(':', '_');
        };
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
        String seed = Bridge.world("seed").trim();
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
            default -> 63;
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
                uid = Bridge.table == null ? UUID.nameUUIDFromBytes(dim.getBytes()) : UUID.randomUUID();
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
        if (s == null) {
            throw new UnsupportedOperationException("chunkwake: spawnEntity " + type);
        }
        return MobImpl.spawn(this, l, s);
    }

    public Entity spawnEntity(Location l, org.bukkit.entity.EntityType type, boolean randomize) {
        return spawnEntity(l, type);
    }

    public Entity spawnEntity(Location l, org.bukkit.entity.EntityType type,
            org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason reason) {
        return spawnEntity(l, type);
    }

    public <T extends Entity> T spawn(Location l, Class<T> type) {
        return spawn(l, type, (java.util.function.Consumer<? super T>) null);
    }

    public <T extends Entity> T spawn(Location l, Class<T> type, org.bukkit.util.Consumer<? super T> init) {
        java.util.function.Consumer<? super T> c = init == null ? null : init::accept;
        return spawn(l, type, c);
    }

    public <T extends Entity> T spawn(Location l, Class<T> type, java.util.function.Consumer<? super T> init) {
        String s = MobImpl.species(type);
        if (s == null) {
            throw new UnsupportedOperationException("chunkwake: spawn " + type.getSimpleName());
        }
        T e = type.cast(MobImpl.spawn(this, l, s));
        if (init != null) {
            init.accept(e);
        }
        return e;
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

    public List<Entity> getEntities() {
        List<Entity> out = new ArrayList<>(getPlayers());
        out.addAll(MobImpl.all(this));
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
