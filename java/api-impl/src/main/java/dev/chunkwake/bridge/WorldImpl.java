package dev.chunkwake.bridge;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.util.ArrayList;
import java.util.Collection;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
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
final class WorldImpl {
    static final String OVERWORLD = "minecraft:overworld";
    static final String NETHER = "minecraft:the_nether";
    static final String END = "minecraft:the_end";

    final World api = Stub.of(World.class, this);
    private final ChunkwakeServer server;
    /** Ключ измерения, например {@code minecraft:the_nether}. */
    final String dim;
    private volatile UUID uid;
    private volatile PdcImpl pdc;

    WorldImpl(ChunkwakeServer server, String dim) {
        this.server = server;
        this.dim = dim;
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

    static WorldImpl of(World w) {
        return Stub.impl(w) instanceof WorldImpl wi ? wi : null;
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
        return new Location(api, 0, 64, 0);
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
        return Stub.of(org.bukkit.entity.Item.class, new BlockEvents.GroundItem(l.clone(), s.clone()));
    }

    @Override
    public String toString() {
        return "CraftWorld{name=" + getName() + "}";
    }
}
