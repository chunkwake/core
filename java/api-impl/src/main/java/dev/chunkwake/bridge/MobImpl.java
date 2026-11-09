package dev.chunkwake.bridge;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.UUID;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer;
import org.bukkit.DyeColor;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.entity.Entity;
import org.bukkit.entity.EntityType;
import org.bukkit.entity.LivingEntity;
import org.bukkit.event.entity.CreatureSpawnEvent;
import org.bukkit.inventory.ItemStack;
import org.bukkit.plugin.PluginManager;

/**
 * Мирный моб (корова, свинья, овца, курица) поверх запросов {@code mob …} к миру.
 * Состояние не кэшируется: каждый геттер спрашивает Rust, кроме мёртвых —
 * у них остаётся последний снимок.
 */
final class MobImpl {
    final int id;
    final String species;
    final LivingEntity api;
    /** Последний снимок {@code mob get}: вид uuid x y z yaw pitch здоровье макс возраст стрижена цвет мёртв любовь. */
    private String[] last;
    private String lastName = "";

    private MobImpl(int id, String species) {
        this.id = id;
        this.species = species;
        this.api = (LivingEntity) Stub.of(iface(species), this);
    }

    static Class<? extends LivingEntity> iface(String species) {
        return switch (species) {
            case "cow" -> org.bukkit.entity.Cow.class;
            case "pig" -> org.bukkit.entity.Pig.class;
            case "sheep" -> org.bukkit.entity.Sheep.class;
            case "chicken" -> org.bukkit.entity.Chicken.class;
            case "zombie" -> org.bukkit.entity.Zombie.class;
            case "creeper" -> org.bukkit.entity.Creeper.class;
            case "skeleton" -> org.bukkit.entity.Skeleton.class;
            case "spider" -> org.bukkit.entity.Spider.class;
            default -> throw new IllegalArgumentException("chunkwake: no mob " + species);
        };
    }

    /** Вид для {@link EntityType}; {@code null} — такого моба сервер не умеет. */
    static String species(EntityType type) {
        return switch (type) {
            case COW, PIG, SHEEP, CHICKEN, ZOMBIE, CREEPER, SKELETON, SPIDER -> type.name().toLowerCase(Locale.ROOT);
            default -> null;
        };
    }

    static String species(Class<?> type) {
        for (String s : new String[] {"cow", "pig", "sheep", "chicken", "zombie", "creeper", "skeleton", "spider"}) {
            if (type.isAssignableFrom(iface(s)) && iface(s).isAssignableFrom(type)) {
                return s;
            }
        }
        return null;
    }

    static LivingEntity of(int id, String species) {
        return new MobImpl(id, species).api;
    }

    /** Новый моб плагина ({@code World.spawnEntity}). */
    static LivingEntity spawn(Location l, String species) {
        String id = Bridge.world("mob spawn " + species + " " + l.getX() + " " + l.getY() + " " + l.getZ() + " " + l.getYaw()).trim();
        if (id.isEmpty()) {
            throw new IllegalStateException("chunkwake: spawn " + species + " failed");
        }
        return of(Integer.parseInt(id), species);
    }

    /** Все живые мобы мира. */
    static List<LivingEntity> all() {
        List<LivingEntity> out = new ArrayList<>();
        String after = String.valueOf(Integer.MIN_VALUE);
        while (true) {
            String page = Bridge.world("mob list " + after);
            if (page.isEmpty()) {
                return out;
            }
            for (String line : page.split("\n")) {
                String[] f = line.split(" ");
                if (f.length >= 2) {
                    out.add(of(Integer.parseInt(f[0]), f[1]));
                    after = f[0];
                }
            }
        }
    }

    private String[] state() {
        String r = Bridge.world("mob get " + id);
        if (!r.isEmpty()) {
            int nl = r.indexOf('\n');
            last = (nl < 0 ? r : r.substring(0, nl)).split(" ");
            lastName = nl < 0 ? "" : r.substring(nl + 1);
        }
        return last;
    }

    private boolean set(String what) {
        return Bridge.world("mob set " + id + " " + what).trim().equals("1");
    }

    private double num(int i) {
        String[] s = state();
        return s == null ? 0 : Double.parseDouble(s[i]);
    }

    public int getEntityId() { return id; }
    public UUID getUniqueId() {
        String[] s = state();
        return s == null ? new UUID(0, id) : UUID.fromString(s[1]);
    }
    public EntityType getType() { return EntityType.valueOf(species.toUpperCase(Locale.ROOT)); }
    public World getWorld() { return Bridge.server.world; }
    public org.bukkit.Server getServer() { return Bridge.server.api; }
    public String getName() {
        String n = getCustomName();
        return n != null ? n : Character.toUpperCase(species.charAt(0)) + species.substring(1);
    }
    public Component name() { return Component.text(getName()); }

    /** Планировщик задач этого моба: на Folia — поток региона, у нас общий. */
    public io.papermc.paper.threadedregions.scheduler.EntityScheduler getScheduler() {
        return Bridge.server.entityScheduler();
    }

    public Location getLocation() {
        String[] s = state();
        if (s == null) {
            return new Location(getWorld(), 0, 0, 0);
        }
        return new Location(getWorld(), num(2), num(3), num(4), (float) num(5), (float) num(6));
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
    public Location getEyeLocation() { return getLocation().add(0, getEyeHeight(), 0); }
    public double getEyeHeight() {
        return switch (species) {
            case "chicken" -> 0.644;
            case "pig" -> 0.765;
            case "sheep" -> 1.235;
            case "zombie", "skeleton" -> 1.74;
            case "creeper" -> 1.445;
            case "spider" -> 0.65;
            default -> 1.3;
        } * (isAdult() ? 1 : 0.5);
    }
    public boolean teleport(Location to) {
        return set("pos " + to.getX() + " " + to.getY() + " " + to.getZ() + " " + to.getYaw() + " " + to.getPitch());
    }
    public boolean teleport(Location to, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause cause) { return teleport(to); }
    public boolean teleport(Entity target) { return teleport(target.getLocation()); }

    public double getHealth() { return num(7); }
    public void setHealth(double h) {
        if (h < 0 || h > getMaxHealth()) {
            throw new IllegalArgumentException("Health value (" + h + ") must be between 0 and " + getMaxHealth());
        }
        set("health " + h);
    }
    public double getMaxHealth() { return num(8); }
    public void damage(double amount) { set("damage " + amount); }
    public void damage(double amount, Entity source) { damage(amount); }
    public boolean isDead() {
        String[] s = Bridge.world("mob get " + id).split(" ");
        return s.length < 13 || s[12].equals("1");
    }
    public boolean isValid() { return !isDead(); }
    public void remove() { set("remove"); }

    public int getAge() { return (int) num(9); }
    public void setAge(int age) { set("age " + age); }
    public boolean isAdult() { return getAge() >= 0; }
    public void setAdult() {
        if (!isAdult()) {
            setAge(0);
        }
    }
    public void setBaby() {
        if (isAdult()) {
            setAge(-24000);
        }
    }
    public boolean canBreed() { return getAge() == 0; }
    public int getLoveModeTicks() { return (int) num(13); }
    public void setLoveModeTicks(int ticks) { set("love " + ticks); }
    public boolean isLoveMode() { return getLoveModeTicks() > 0; }

    public boolean isSheared() { return num(10) != 0; }
    public void setSheared(boolean v) { set("sheared " + (v ? 1 : 0)); }
    @SuppressWarnings("deprecation")
    public DyeColor getColor() { return DyeColor.getByWoolData((byte) num(11)); }
    @SuppressWarnings("deprecation")
    public void setColor(DyeColor c) { set("color " + c.getWoolData()); }

    public String getCustomName() {
        state();
        return lastName.isEmpty() ? null : lastName;
    }
    public void setCustomName(String name) { set("name " + (name == null ? "" : name)); }
    public Component customName() {
        String n = getCustomName();
        return n == null ? null : Component.text(n);
    }
    public void customName(Component c) {
        setCustomName(c == null ? null : PlainTextComponentSerializer.plainText().serialize(c));
    }
    public boolean isCustomNameVisible() { return getCustomName() != null; }
    public void setCustomNameVisible(boolean v) {}

    @Override
    public boolean equals(Object o) {
        return (o instanceof MobImpl m ? m : Stub.impl(o)) instanceof MobImpl m && m.id == id;
    }

    @Override
    public int hashCode() { return id; }

    @Override
    public String toString() { return "Craft" + getName() + "{id=" + id + "}"; }

    // --- события ---

    /** Моб из аргумента события: {@code id вид …}. */
    private static LivingEntity mob(String[] f) {
        return of(Integer.parseInt(f[0]), f[1]);
    }

    /** События мобов без игрока: смерть, спавн, размножение. */
    static String fire(PluginManager plugins, int kind, String arg) {
        return switch (kind) {
            case Bridge.EVENT_MOB_DEATH -> {
                String[] lines = arg.split("\n");
                String[] f = lines[0].split(" ");
                List<ItemStack> drops = new ArrayList<>();
                for (int i = 1; i < lines.length; i++) {
                    drops.add(StackImpl.parse(lines[i]));
                }
                int xp = Integer.parseInt(f[5]);
                var e = new org.bukkit.event.entity.EntityDeathEvent(mob(f), Events.source("minecraft:generic"),
                        new ArrayList<>(drops), xp);
                plugins.callEvent(e);
                StringBuilder out = new StringBuilder().append(e.getDroppedExp());
                for (ItemStack s : e.getDrops()) {
                    String line = StackImpl.line(s);
                    if (line != null) {
                        out.append('\n').append(line);
                    }
                }
                yield out.toString();
            }
            case Bridge.EVENT_CREATURE_SPAWN -> {
                String[] f = arg.split(" ");
                var e = new CreatureSpawnEvent(mob(f), CreatureSpawnEvent.SpawnReason.valueOf(f[5]));
                plugins.callEvent(e);
                yield e.isCancelled() ? "c" : "";
            }
            case Bridge.EVENT_BREED -> {
                String[] f = arg.split(" ");
                var child = Bridge.world("mob get " + f[0]).split(" ");
                String kind2 = child.length > 1 ? child[0] : "cow";
                var e = new org.bukkit.event.entity.EntityBreedEvent(of(Integer.parseInt(f[0]), kind2),
                        of(Integer.parseInt(f[1]), kind2), of(Integer.parseInt(f[2]), kind2), null, null,
                        Integer.parseInt(f[3]));
                plugins.callEvent(e);
                yield e.isCancelled() ? "c" : String.valueOf(e.getExperience());
            }
            default -> "";
        };
    }

    /** События мобов от игрока: удар, правый клик, стрижка. */
    static String fire(PluginManager plugins, PlayerImpl p, int kind, String arg) {
        String[] f = arg.split(" ");
        return switch (kind) {
            case Bridge.EVENT_MOB_DAMAGE -> {
                double dmg = Double.parseDouble(f[2]);
                var src = Events.source("minecraft:player_attack");
                var e = new org.bukkit.event.entity.EntityDamageByEntityEvent(p.api, mob(f),
                        org.bukkit.event.entity.EntityDamageEvent.DamageCause.ENTITY_ATTACK, src, dmg);
                plugins.callEvent(e);
                yield e.isCancelled() ? "c" : e.getDamage() != dmg ? "d " + e.getDamage() : "";
            }
            case Bridge.EVENT_INTERACT_ENTITY -> {
                var e = new org.bukkit.event.player.PlayerInteractEntityEvent(p.api, mob(f),
                        org.bukkit.inventory.EquipmentSlot.valueOf(f[2]));
                plugins.callEvent(e);
                yield e.isCancelled() ? "c" : "";
            }
            case Bridge.EVENT_SHEAR -> {
                var mob = of(Integer.parseInt(f[0]), "sheep");
                var hand = org.bukkit.inventory.EquipmentSlot.valueOf(f[1]);
                var e = new org.bukkit.event.player.PlayerShearEntityEvent(p.api, mob,
                        p.api.getInventory().getItem(hand), hand, new ArrayList<>());
                plugins.callEvent(e);
                yield e.isCancelled() ? "c" : "";
            }
            default -> "";
        };
    }
}
