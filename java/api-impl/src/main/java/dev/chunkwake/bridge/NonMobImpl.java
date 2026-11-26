package dev.chunkwake.bridge;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.UUID;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.entity.Entity;
import org.bukkit.entity.EntityType;

/**
 * Немобовая сущность (стрела, трезубец, ТНТ, сфера опыта, облако зелья, глаз
 * Края, лодка/плот, вагонетка) поверх запросов {@code entity …} к миру.
 *
 * <p>Плагины видят только те виды, которые Rust уже симулирует. Состояние не
 * кэшируется: геттеры спрашивают Rust, кроме удалённых сущностей — у них
 * остаётся только признак {@link #removed}.
 *
 * <p>UUID выводится сервером из ID сущности детерминированно. У лодок и
 * вагонеток он не совпадает с UUID, который ушёл клиентам при спавне.
 */
final class NonMobImpl {
    /** Виды, у которых есть серверный хелпер `summon_*` в Rust. */
    private static final EntityType[] DIRECT = {
        EntityType.ARROW,
        EntityType.TRIDENT,
        EntityType.TNT,
        EntityType.EXPERIENCE_ORB,
        EntityType.AREA_EFFECT_CLOUD,
        EntityType.EYE_OF_ENDER,
        EntityType.MINECART,
        EntityType.CHEST_MINECART,
        EntityType.HOPPER_MINECART,
        EntityType.FURNACE_MINECART,
        EntityType.TNT_MINECART,
    };

    /** Все ключи {@code EntityType} — для разбора строк {@code entity list}. */
    private static final java.util.Map<String, EntityType> BY_KEY = byKey();

    final WorldImpl world;
    final int id;
    final String kind;
    final UUID uuid;
    final EntityType type;
    final Entity api;
    /** Последний снимок {@code entity get}: вид uuid x y z yaw pitch. */
    private volatile String[] last;
    /** Сущность была в последнем ответе Rust (снимок может остаться и после удаления). */
    private volatile boolean present = true;
    private volatile boolean removed;

    private NonMobImpl(WorldImpl world, int id, String kind, UUID uuid, EntityType type) {
        this.world = world;
        this.id = id;
        this.kind = kind;
        this.uuid = uuid;
        this.type = type;
        Class<? extends Entity> cls = type.getEntityClass();
        if (cls == null) {
            throw new UnsupportedOperationException("chunkwake: spawn " + type);
        }
        this.api = (Entity) Stub.ofAll(
                new Class<?>[] {cls, org.bukkit.craftbukkit.entity.CraftEntity.class}, this);
    }

    /** {@link EntityType} поддерживается, если Rust умеет его создать. */
    static boolean supported(EntityType type) {
        if (type == null) {
            return false;
        }
        for (EntityType direct : DIRECT) {
            if (direct == type) {
                return true;
            }
        }
        String key = key(type);
        return key != null && (key.endsWith("_boat") || key.endsWith("_raft"));
    }

    /** Имя типа для {@code entity spawn}; {@code null} — вид не поддерживается. */
    static String kind(EntityType type) {
        return supported(type) ? key(type) : null;
    }

    /**
     * Вид по Bukkit-классу. Сначала точное совпадение (включая конкретные
     * интерфейсы вроде {@code OakBoat}), затем общие {@code Boat},
     * {@code ChestBoat} и {@code Minecart} — им соответствует ванильный вид
     * по умолчанию. Для слишком общих классов ({@code Entity}, {@code Vehicle})
     * вид не угадывается.
     */
    static EntityType type(Class<?> cls) {
        if (cls == null) {
            return null;
        }
        if (cls == org.bukkit.entity.Boat.class) {
            return EntityType.OAK_BOAT;
        }
        if (cls == org.bukkit.entity.ChestBoat.class) {
            return EntityType.OAK_CHEST_BOAT;
        }
        if (cls == org.bukkit.entity.Minecart.class) {
            return EntityType.MINECART;
        }
        for (EntityType type : EntityType.values()) {
            if (!supported(type)) {
                continue;
            }
            Class<? extends Entity> candidate = type.getEntityClass();
            if (candidate != null && candidate == cls) {
                return type;
            }
        }
        // Общий Vehicle/Entity не угадываем: для них нужен конкретный вид.
        return null;
    }

    /** Новый вид плагина ({@code World.spawnEntity}/{@code World.spawn}). */
    static Entity spawn(WorldImpl world, Location l, EntityType type) {
        String kind = kind(type);
        if (kind == null) {
            throw new UnsupportedOperationException("chunkwake: spawnEntity " + type);
        }
        String reply = world.request("entity spawn " + kind + " " + l.getX() + " " + l.getY()
                + " " + l.getZ() + " " + l.getYaw()).trim();
        if (reply.isEmpty()) {
            throw new IllegalStateException("chunkwake: spawn " + kind + " failed");
        }
        String[] f = reply.split(" ");
        if (f.length < 2) {
            throw new IllegalStateException("chunkwake: spawn " + kind + " reply: " + reply);
        }
        UUID uuid;
        try {
            uuid = UUID.fromString(f[1]);
        } catch (IllegalArgumentException e) {
            throw new IllegalStateException("chunkwake: spawn " + kind + " uuid: " + f[1], e);
        }
        return new NonMobImpl(world, Integer.parseInt(f[0]), kind, uuid, type).api;
    }

    /** Обёртка уже созданной Rust-сущности (например, стрелы из {@code World.spawnArrow}). */
    static Entity wrap(WorldImpl world, int id, String kind, UUID uuid, EntityType type) {
        return new NonMobImpl(world, id, kind, uuid, type).api;
    }

    /** Все поддерживаемые немобовые сущности мира, страницами до 7 КБ. */
    static List<Entity> all(WorldImpl world) {
        List<Entity> out = new ArrayList<>();
        String after = String.valueOf(Integer.MIN_VALUE);
        while (true) {
            String page = world.request("entity list " + after);
            if (page.isEmpty()) {
                return out;
            }
            boolean advanced = false;
            for (String line : page.split("\n")) {
                String[] f = line.split(" ");
                if (f.length < 3) {
                    continue;
                }
                int id;
                try {
                    id = Integer.parseInt(f[0]);
                } catch (IllegalArgumentException ignored) {
                    continue;
                }
                after = f[0];
                advanced = true;
                UUID uuid;
                EntityType type;
                try {
                    uuid = UUID.fromString(f[2]);
                    type = byKind(f[1]);
                } catch (IllegalArgumentException ignored) {
                    continue;
                }
                if (type == null || !supported(type)) {
                    // Чужой вид не должен ломать перечисление мира.
                    continue;
                }
                out.add(new NonMobImpl(world, id, f[1], uuid, type).api);
            }
            if (!advanced) {
                return out;
            }
        }
    }

    private static String key(EntityType type) {
        try {
            return type.getKey().toString();
        } catch (RuntimeException e) {
            return null;
        }
    }

    private static java.util.Map<String, EntityType> byKey() {
        java.util.Map<String, EntityType> map = new java.util.HashMap<>();
        for (EntityType type : EntityType.values()) {
            String key = key(type);
            if (key != null) {
                map.putIfAbsent(key, type);
            }
        }
        return map;
    }

    private static EntityType byKind(String kind) {
        return BY_KEY.get(kind);
    }

    /** Свежий снимок; {@code null} — сущности уже нет. */
    private String[] refresh() {
        String r = world.request("entity get " + id).trim();
        if (r.isEmpty()) {
            present = false;
            return null;
        }
        last = r.split(" ");
        present = true;
        return last;
    }

    /** Снимок для позиции: после удаления остаётся последний известный. */
    private String[] state() {
        String[] s = refresh();
        return s != null ? s : last;
    }

    /**
     * `entity get`: `<id> <kind> <uuid> <x> <y> <z> <yaw> <pitch>`.
     * Битая строка не должна ронять вызов плагина — отдаём начало координат.
     */
    static Location location(World world, String[] s) {
        if (s == null || s.length < 8) {
            return new Location(world, 0, 0, 0);
        }
        try {
            return new Location(world, Double.parseDouble(s[3]), Double.parseDouble(s[4]),
                    Double.parseDouble(s[5]), (float) Double.parseDouble(s[6]),
                    (float) Double.parseDouble(s[7]));
        } catch (NumberFormatException e) {
            return new Location(world, 0, 0, 0);
        }
    }

    public int getEntityId() { return id; }

    public UUID getUniqueId() { return uuid; }

    public EntityType getType() { return type; }

    public World getWorld() { return world.api; }

    public org.bukkit.Server getServer() { return Bridge.server.api; }

    public Location getLocation() {
        return location(getWorld(), state());
    }

    public org.bukkit.util.Vector getVelocity() {
        String reply = world.request("entity velocity " + id).trim();
        String[] f = reply.split(" ");
        if (f.length < 3) {
            return new org.bukkit.util.Vector();
        }
        try {
            return new org.bukkit.util.Vector(Double.parseDouble(f[0]),
                    Double.parseDouble(f[1]), Double.parseDouble(f[2]));
        } catch (NumberFormatException e) {
            return new org.bukkit.util.Vector();
        }
    }

    public void setVelocity(org.bukkit.util.Vector v) {
        if (v == null) {
            throw new IllegalArgumentException("Velocity cannot be null");
        }
        world.request("entity setvelocity " + id + " "
                + v.getX() + " " + v.getY() + " " + v.getZ());
    }

    public Location getLocation(Location into) {
        Location l = getLocation();
        if (into != null) {
            into.setWorld(l.getWorld());
            into.setX(l.getX());
            into.setY(l.getY());
            into.setZ(l.getZ());
            into.setYaw(l.getYaw());
            into.setPitch(l.getPitch());
        }
        return into;
    }

    public boolean isDead() {
        if (removed) {
            return true;
        }
        refresh();
        return !present;
    }

    public boolean isValid() { return !isDead(); }

    public void remove() {
        if (removed) {
            return;
        }
        removed = true;
        world.request("entity remove " + id);
    }

    public String getName() { return type.getName(); }

    /** NMS-представление сущности для CraftBukkit-прокси, как у мобов. */
    private net.minecraft.world.entity.Entity handle;

    public net.minecraft.world.entity.Entity getHandle() {
        if (this.handle == null) {
            net.minecraft.resources.Identifier id =
                    net.minecraft.resources.Identifier.parse(this.kind);
            Object registered = net.minecraft.core.registries.BuiltInRegistries.ENTITY_TYPE.getValue(id);
            net.minecraft.world.entity.EntityType<?> nms;
            if (registered instanceof net.minecraft.world.entity.EntityType<?> existing) {
                nms = existing;
            } else {
                nms = new net.minecraft.world.entity.EntityType<>(id.toString());
                net.minecraft.core.registries.BuiltInRegistries.ENTITY_TYPE.register(id, nms);
            }
            this.handle = new net.minecraft.world.entity.Entity(nms, null, getUniqueId());
        }
        return this.handle;
    }

    public void setHandle(Object value) {
        this.handle = value instanceof net.minecraft.world.entity.Entity entity ? entity : null;
    }

    @Override
    public boolean equals(Object o) {
        return (o instanceof NonMobImpl e ? e : Stub.impl(o)) instanceof NonMobImpl e && e.id == id;
    }

    @Override
    public int hashCode() { return id; }

    @Override
    public String toString() {
        return "Craft" + type.name().toLowerCase(Locale.ROOT) + "{id=" + id + "}";
    }
}
