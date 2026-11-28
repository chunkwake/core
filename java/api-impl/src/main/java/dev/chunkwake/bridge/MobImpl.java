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

    /** Мир моба: запросы `mob …` идут в его измерение. */
    final WorldImpl world;

    private MobImpl(WorldImpl world, int id, String species) {
        this.world = world;
        this.id = id;
        this.species = species;
        this.api = (LivingEntity) Stub.ofAll(new Class<?>[] {iface(species), org.bukkit.craftbukkit.entity.CraftEntity.class}, this);
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
            case "slime" -> org.bukkit.entity.Slime.class;
            default -> fallbackIface(species);
        };
    }

    /**
     * Любой вид, которого нет в списке выше: берём настоящий Bukkit-интерфейс
     * из {@code EntityType.getEntityClass()} (Piglin, Hoglin, Blaze, ...).
     * Если интерфейс неизвестен — общий {@link LivingEntity}, но без падения:
     * перечисление сущностей мира (ImageFrame/WorldEdit) не должно ломаться.
     */
    @SuppressWarnings("unchecked")
    private static Class<? extends LivingEntity> fallbackIface(String species) {
        try {
            org.bukkit.entity.EntityType type = org.bukkit.entity.EntityType
                    .valueOf(species.toUpperCase(java.util.Locale.ROOT));
            Class<? extends org.bukkit.entity.Entity> cls = type.getEntityClass();
            if (cls != null && LivingEntity.class.isAssignableFrom(cls)) {
                return (Class<? extends LivingEntity>) cls;
            }
        } catch (IllegalArgumentException ignored) {
            // вид без EntityType — общий интерфейс
        }
        return LivingEntity.class;
    }

    /**
     * Все виды, которые умеет Rust (`Species::ALL`). `end_crystal` сюда не
     * входит: это не живая сущность, и `MobImpl` не может её создать.
     */
    private static final String[] SPECIES = {
        "cow", "pig", "sheep", "chicken", "zombie", "creeper", "skeleton",
        "spider", "slime", "drowned", "piglin", "wolf", "cod", "iron_golem",
        "phantom", "pillager", "vindicator", "evoker", "vex", "witch",
        "ravager", "wither", "snow_golem", "copper_golem", "ender_dragon",
        "parrot", "warden", "cat", "bat", "salmon", "glow_squid",
        "bee", "squid", "dolphin", "tropical_fish", "pufferfish",
        "axolotl",
        "donkey",
        "blaze",
        "wither_skeleton",
        "magma_cube",
        "guardian",
        "zombified_piglin",
    };

    /** Вид для {@link EntityType}; {@code null} — такого моба сервер не умеет. */
    static String species(EntityType type) {
        if (type == null) {
            return null;
        }
        String name = type.name().toLowerCase(Locale.ROOT);
        for (String species : SPECIES) {
            if (species.equals(name)) {
                return species;
            }
        }
        return null;
    }

    static String species(Class<?> type) {
        for (String s : SPECIES) {
            if (type.isAssignableFrom(iface(s)) && iface(s).isAssignableFrom(type)) {
                return s;
            }
        }
        return null;
    }

    /** Моб из события тика: мобы событий пока только оверворлда. */
    static LivingEntity of(int id, String species) {
        return of(Bridge.server.overworld, id, species);
    }

    static LivingEntity of(WorldImpl world, int id, String species) {
        return new MobImpl(world, id, species).api;
    }

    /** Новый моб плагина ({@code World.spawnEntity}). */
    static LivingEntity spawn(WorldImpl world, Location l, String species) {
        String id = world.request("mob spawn " + species + " " + l.getX() + " " + l.getY() + " " + l.getZ() + " " + l.getYaw()).trim();
        if (id.isEmpty()) {
            throw new IllegalStateException("chunkwake: spawn " + species + " failed");
        }
        return of(world, Integer.parseInt(id), species);
    }

    /** Все живые мобы мира. */
    static List<LivingEntity> all(WorldImpl world) {
        List<LivingEntity> out = new ArrayList<>();
        String after = String.valueOf(Integer.MIN_VALUE);
        while (true) {
            String page = world.request("mob list " + after);
            if (page.isEmpty()) {
                return out;
            }
            for (String line : page.split("\n")) {
                String[] f = line.split(" ");
                if (f.length >= 2) {
                    out.add(of(world, Integer.parseInt(f[0]), f[1]));
                    after = f[0];
                }
            }
        }
    }

    private String[] state() {
        String r = world.request("mob get " + id);
        if (!r.isEmpty()) {
            int nl = r.indexOf('\n');
            last = (nl < 0 ? r : r.substring(0, nl)).split(" ");
            lastName = nl < 0 ? "" : r.substring(nl + 1);
        }
        return last;
    }

    private boolean set(String what) {
        return world.request("mob set " + id + " " + what).trim().equals("1");
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
    /** Атрибут моба: набор вида, как у `/attribute` (`null` — у вида его нет). */
    public org.bukkit.attribute.AttributeInstance getAttribute(org.bukkit.attribute.Attribute a) {
        return AttributeInstanceImpl.of(this::getUniqueId, a);
    }
    public void registerAttribute(org.bukkit.attribute.Attribute a) {}
    public EntityType getType() { return EntityType.valueOf(species.toUpperCase(Locale.ROOT)); }
    public World getWorld() { return world.api; }

    /** NMS-представление сущности для CraftEngine/CraftBukkit-прокси. */
    private net.minecraft.world.entity.Entity handle;

    public net.minecraft.world.entity.Entity getHandle() {
        if (this.handle == null) {
            net.minecraft.resources.Identifier id =
                    net.minecraft.resources.Identifier.parse("minecraft:" + this.species);
            Object registered = net.minecraft.core.registries.BuiltInRegistries.ENTITY_TYPE.getValue(id);
            net.minecraft.world.entity.EntityType<?> type;
            if (registered instanceof net.minecraft.world.entity.EntityType<?> existing) {
                type = existing;
            } else {
                type = new net.minecraft.world.entity.EntityType<>(id.toString());
                net.minecraft.core.registries.BuiltInRegistries.ENTITY_TYPE.register(id, type);
            }
            this.handle = new net.minecraft.world.entity.Entity(type, null, getUniqueId());
        }
        return this.handle;
    }

    public void setHandle(Object value) {
        this.handle = value instanceof net.minecraft.world.entity.Entity entity ? entity : null;
    }

    private volatile PdcImpl pdc;

    /** {@code Entity#getScoreboardTags}: NBT {@code Tags}. */
    public java.util.Set<String> getScoreboardTags() {
        java.util.Set<String> out = new java.util.LinkedHashSet<>();
        String json = world.request("mob tags " + id);
        try {
            com.google.gson.JsonArray array =
                    com.google.gson.JsonParser.parseString(json).getAsJsonArray();
            for (com.google.gson.JsonElement tag : array) {
                out.add(tag.getAsString());
            }
        } catch (RuntimeException ignored) {
            // пустой ответ — тегов нет
        }
        return out;
    }

    public boolean addScoreboardTag(String tag) {
        if (tag == null || tag.isEmpty()) {
            return false;
        }
        return "1".equals(world.request("mob tag " + id + " add " + tag));
    }

    public boolean removeScoreboardTag(String tag) {
        if (tag == null) {
            return false;
        }
        return "1".equals(world.request("mob tag " + id + " remove " + tag));
    }

    /** {@code Entity#getPersistentDataContainer}: NBT {@code BukkitValues}. */
    public org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer() {
        PdcImpl p = pdc;
        if (p == null) {
            synchronized (this) {
                if (pdc == null) {
                    java.util.Map<String, Object> tags = new java.util.LinkedHashMap<>();
                    String json = world.request("mob pdc " + id);
                    if (!json.isEmpty()) {
                        tags.putAll(PdcImpl.Nbt.decode(
                                com.google.gson.JsonParser.parseString(json).getAsJsonObject()));
                    }
                    pdc = new PdcImpl(tags,
                            t -> world.request("mob pdc " + id + " " + PdcImpl.Nbt.encode(t)));
                }
                p = pdc;
            }
        }
        return p.api;
    }

    public void setMetadata(String key, org.bukkit.metadata.MetadataValue value) {
        Bridge.server.metadata().entities.setMetadata(api, key, value);
    }
    public List<org.bukkit.metadata.MetadataValue> getMetadata(String key) {
        return Bridge.server.metadata().entities.getMetadata(api, key);
    }
    public boolean hasMetadata(String key) { return Bridge.server.metadata().entities.hasMetadata(api, key); }
    public void removeMetadata(String key, org.bukkit.plugin.Plugin owner) {
        Bridge.server.metadata().entities.removeMetadata(api, key, owner);
    }
    public org.bukkit.Server getServer() { return Bridge.server.api; }
    public String getName() {
        String n = getCustomName();
        return n != null ? n : Character.toUpperCase(species.charAt(0)) + species.substring(1);
    }
    public Component name() { return Component.text(getName()); }

    private io.papermc.paper.threadedregions.scheduler.EntityScheduler scheduler;

    /** Планировщик задач этого моба: на Folia — поток региона, у нас общий. */
    public io.papermc.paper.threadedregions.scheduler.EntityScheduler getScheduler() {
        if (scheduler == null) {
            scheduler = Bridge.server.folia().entity(() -> !isValid());
        }
        return scheduler;
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
            case "slime" -> 0.325 * getSize();
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

    private String[] tameState() {
        return world.request("mob tamed " + id).trim().split(" ");
    }

    public boolean isTamed() {
        String[] f = tameState();
        return f.length > 0 && f[0].equals("1");
    }

    public void setTamed(boolean tamed) {
        set("tamed " + (tamed ? "1" : "0"));
    }

    public java.util.UUID getOwnerUniqueId() {
        String[] f = tameState();
        if (f.length < 2 || f[1].equals("-")) {
            return null;
        }
        try {
            return java.util.UUID.fromString(f[1]);
        } catch (IllegalArgumentException e) {
            return null;
        }
    }

    public org.bukkit.entity.AnimalTamer getOwner() {
        java.util.UUID owner = getOwnerUniqueId();
        return owner == null ? null : Bridge.server.getOfflinePlayer(owner);
    }

    public void setOwner(org.bukkit.entity.AnimalTamer owner) {
        set(owner == null ? "owner -" : "owner " + owner.getUniqueId());
    }

    public boolean isSitting() {
        return world.request("mob sitting " + id).trim().equals("1");
    }

    public void setSitting(boolean sitting) {
        set("sitting " + (sitting ? "1" : "0"));
    }
    public double getMaxHealth() { return num(8); }

    /** Слот `EquipmentSlot` → имя в запросах Rust; {@code null} — нет такого. */
    private static String gearSlot(org.bukkit.inventory.EquipmentSlot slot) {
        if (slot == null) {
            return null;
        }
        return switch (slot) {
            case HAND -> "mainhand";
            case OFF_HAND -> "offhand";
            case HEAD -> "head";
            case CHEST -> "chest";
            case LEGS -> "legs";
            case FEET -> "feet";
            default -> null;
        };
    }

    private ItemStack gear(String slot) {
        String line = world.request("mob " + id + " equip " + slot).trim();
        return line.isEmpty() ? null : ItemBridge.parse(line);
    }

    private void setGear(String slot, ItemStack item) {
        world.request("mob " + id + " setequip " + slot
                + (item == null ? "" : " " + ItemBridge.line(item)));
    }

    private float dropChance(String slot) {
        String value = world.request("mob " + id + " dropchance " + slot).trim();
        try {
            return Float.parseFloat(value);
        } catch (NumberFormatException e) {
            return 0.0f;
        }
    }

    private void setDropChance(String slot, float chance) {
        world.request("mob " + id + " setdropchance " + slot + " " + chance);
    }

    public org.bukkit.inventory.EntityEquipment getEquipment() {
        return (org.bukkit.inventory.EntityEquipment) Stub.of(
                org.bukkit.inventory.EntityEquipment.class, this);
    }

    public ItemStack getItem(org.bukkit.inventory.EquipmentSlot slot) {
        String name = gearSlot(slot);
        return name == null ? null : gear(name);
    }

    public void setItem(org.bukkit.inventory.EquipmentSlot slot, ItemStack item) {
        String name = gearSlot(slot);
        if (name != null) {
            setGear(name, item);
        }
    }

    public ItemStack getItemInMainHand() { return gear("mainhand"); }

    public void setItemInMainHand(ItemStack item) { setGear("mainhand", item); }

    public ItemStack getItemInOffHand() { return gear("offhand"); }

    public void setItemInOffHand(ItemStack item) { setGear("offhand", item); }

    public ItemStack getItemInHand() { return getItemInMainHand(); }

    public void setItemInHand(ItemStack item) { setItemInMainHand(item); }

    public ItemStack getHelmet() { return gear("head"); }

    public void setHelmet(ItemStack item) { setGear("head", item); }

    public ItemStack getChestplate() { return gear("chest"); }

    public void setChestplate(ItemStack item) { setGear("chest", item); }

    public ItemStack getLeggings() { return gear("legs"); }

    public void setLeggings(ItemStack item) { setGear("legs", item); }

    public ItemStack getBoots() { return gear("feet"); }

    public void setBoots(ItemStack item) { setGear("feet", item); }

    public ItemStack[] getArmorContents() {
        return new ItemStack[] {getBoots(), getLeggings(), getChestplate(), getHelmet()};
    }

    public void setArmorContents(ItemStack[] items) {
        if (items == null) {
            setArmorContents(new ItemStack[4]);
            return;
        }
        setBoots(items.length > 0 ? items[0] : null);
        setLeggings(items.length > 1 ? items[1] : null);
        setChestplate(items.length > 2 ? items[2] : null);
        setHelmet(items.length > 3 ? items[3] : null);
    }

    public float getItemInMainHandDropChance() { return dropChance("mainhand"); }

    public void setItemInMainHandDropChance(float chance) { setDropChance("mainhand", chance); }

    public float getItemInOffHandDropChance() { return dropChance("offhand"); }

    public void setItemInOffHandDropChance(float chance) { setDropChance("offhand", chance); }

    public float getHelmetDropChance() { return dropChance("head"); }

    public void setHelmetDropChance(float chance) { setDropChance("head", chance); }

    public float getChestplateDropChance() { return dropChance("chest"); }

    public void setChestplateDropChance(float chance) { setDropChance("chest", chance); }

    public float getLeggingsDropChance() { return dropChance("legs"); }

    public void setLeggingsDropChance(float chance) { setDropChance("legs", chance); }

    public float getBootsDropChance() { return dropChance("feet"); }

    public void setBootsDropChance(float chance) { setDropChance("feet", chance); }

    public float getItemInHandDropChance() { return getItemInMainHandDropChance(); }

    public void setItemInHandDropChance(float chance) { setItemInMainHandDropChance(chance); }

    public void damage(double amount) { set("damage " + amount); }
    public void damage(double amount, Entity source) { damage(amount); }
    public boolean isDead() {
        String[] s = world.request("mob get " + id).split(" ");
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

    /** `Slime.getSize`: размер 1..127. */
    public int getSize() { return (int) num(14); }
    public void setSize(int size) { set("size " + size); }

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
            // Paper: отмена или число детей не больше 0 — без деления.
            case Bridge.EVENT_SLIME_SPLIT -> {
                String[] f = arg.split(" ");
                var e = new org.bukkit.event.entity.SlimeSplitEvent(
                        (org.bukkit.entity.AbstractCubeMob) mob(f), Integer.parseInt(f[2]));
                plugins.callEvent(e);
                yield e.isCancelled() ? "c" : String.valueOf(e.getCount());
            }
            case Bridge.EVENT_BREED -> {
                String[] f = arg.split(" ");
                var child = Bridge.server.overworld.request("mob get " + f[0]).split(" ");
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
