package dev.chunkwake.bridge;

import io.papermc.paper.chat.ChatRenderer;
import io.papermc.paper.event.player.AsyncChatEvent;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.Set;
import net.kyori.adventure.audience.Audience;
import net.kyori.adventure.chat.SignedMessage;
import net.kyori.adventure.text.serializer.legacy.LegacyComponentSerializer;
import org.bukkit.Bukkit;
import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;
import org.bukkit.block.BlockState;
import org.bukkit.entity.Player;
import org.bukkit.event.block.Action;
import org.bukkit.event.block.BlockPlaceEvent;
import org.bukkit.event.player.AsyncPlayerChatEvent;
import org.bukkit.event.player.PlayerCommandPreprocessEvent;
import org.bukkit.event.player.PlayerInteractEvent;
import org.bukkit.event.player.PlayerMoveEvent;
import org.bukkit.inventory.EquipmentSlot;
import java.util.List;
import java.util.UUID;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.serializer.gson.GsonComponentSerializer;
import org.bukkit.Location;
import org.bukkit.NamespacedKey;
import org.bukkit.World;
import org.bukkit.damage.DamageSource;
import org.bukkit.damage.DamageType;
import org.bukkit.entity.Entity;
import org.bukkit.entity.Item;
import org.bukkit.event.entity.EntityDamageEvent;
import org.bukkit.event.entity.EntityPickupItemEvent;
import org.bukkit.event.entity.PlayerDeathEvent;
import org.bukkit.event.player.PlayerDropItemEvent;
import org.bukkit.event.player.PlayerGameModeChangeEvent;
import org.bukkit.inventory.ItemStack;
import org.bukkit.plugin.PluginManager;

/**
 * События с ответом из Rust ({@code chunkwake_net::live::Event}): аргумент и
 * ответ — строки, поля через пробел, записи через {@code \n}.
 */
final class Events {
    private Events() {}

    static String fire(PluginManager plugins, PlayerImpl p, int kind, String arg) {
        return switch (kind) {
            case Bridge.EVENT_DAMAGE -> damage(plugins, p, arg);
            case Bridge.EVENT_DEATH -> death(plugins, p, arg);
            case Bridge.EVENT_DROP -> {
                String[] f = arg.split(" ");
                PlayerDropItemEvent e = new PlayerDropItemEvent(p.api, item(p, f[0], Integer.parseInt(f[1])));
                plugins.callEvent(e);
                yield e.isCancelled() ? "c" : "";
            }
            case Bridge.EVENT_PICKUP -> {
                String[] f = arg.split(" ");
                EntityPickupItemEvent e = new EntityPickupItemEvent(p.api, item(p, f[0], Integer.parseInt(f[1])),
                        Integer.parseInt(f[2]));
                plugins.callEvent(e);
                yield e.isCancelled() ? "c" : "";
            }
            case Bridge.EVENT_GAME_MODE -> {
                String[] f = arg.split(" ");
                @SuppressWarnings("deprecation")
                org.bukkit.GameMode mode = org.bukkit.GameMode.getByValue(Integer.parseInt(f[0]));
                PlayerGameModeChangeEvent e = new PlayerGameModeChangeEvent(p.api, mode,
                        PlayerGameModeChangeEvent.Cause.valueOf(f[1]), null);
                plugins.callEvent(e);
                yield e.isCancelled() ? "c" : "";
            }
            case Bridge.EVENT_CHAT -> chat(plugins, p, arg);
            case Bridge.EVENT_COMMAND_PRE -> {
                PlayerCommandPreprocessEvent e = new PlayerCommandPreprocessEvent(p.api, "/" + arg);
                plugins.callEvent(e);
                yield e.isCancelled() ? "c" : e.getMessage().equals("/" + arg) ? "" : "m " + e.getMessage();
            }
            case Bridge.EVENT_PLACE -> place(plugins, p, arg);
            case Bridge.EVENT_MOVE -> move(plugins, p, arg);
            case Bridge.EVENT_INTERACT -> interact(plugins, p, arg);
            case Bridge.EVENT_POTION -> potion(plugins, p, arg);
            case Bridge.EVENT_PREPARE_ENCHANT -> prepareEnchant(plugins, p, arg);
            case Bridge.EVENT_ENCHANT -> enchant(plugins, p, arg);
            default -> "";
        };
    }

    /**
     * Чат как у Paper: сначала legacy {@code AsyncPlayerChatEvent} (сообщение и формат),
     * затем {@code AsyncChatEvent} с рендером → JSON итогового сообщения или {@code c}.
     */
    private static String chat(PluginManager plugins, PlayerImpl p, String text) {
        Component name = Component.text(p.name);
        Set<Player> players = new HashSet<>(Bukkit.getOnlinePlayers());
        AsyncPlayerChatEvent legacy = new AsyncPlayerChatEvent(false, p.api, text, players);
        plugins.callEvent(legacy);
        if (legacy.isCancelled()) {
            return "c";
        }
        Component message = LegacyComponentSerializer.legacySection().deserialize(legacy.getMessage());
        ChatRenderer renderer = ChatRenderer.defaultRenderer();
        if (!legacy.getFormat().equals("<%1$s> %2$s")) {
            String formatted = String.format(legacy.getFormat(), p.name, legacy.getMessage());
            Component fixed = LegacyComponentSerializer.legacySection().deserialize(formatted);
            renderer = (source, display, msg, viewer) -> fixed;
        }
        Set<Audience> viewers = new HashSet<>(players);
        viewers.add(Bukkit.getConsoleSender());
        AsyncChatEvent e = new AsyncChatEvent(false, p.api, viewers, renderer, message, message,
                SignedMessage.system(text, message));
        plugins.callEvent(e);
        if (e.isCancelled()) {
            return "c";
        }
        return GsonComponentSerializer.gson().serialize(e.renderer().render(p.api, name, e.message(), Audience.empty()));
    }

    private static org.bukkit.enchantments.Enchantment enchantment(String key) {
        NamespacedKey k = NamespacedKey.fromString(key);
        return k == null ? null : org.bukkit.Registry.ENCHANTMENT.get(k);
    }

    /**
     * {@code PrepareItemEnchantEvent}: {@code x y z полки}, строка предмета, три
     * {@code цена чары уровень} ({@code -} — нет) → {@code c}, пусто или три новые строки.
     */
    private static String prepareEnchant(PluginManager plugins, PlayerImpl p, String arg) {
        String[] lines = arg.split("\n", -1);
        String[] f = lines[0].split(" ");
        Block table = new BlockImpl(p.getWorld(), Integer.parseInt(f[0]), Integer.parseInt(f[1]),
                Integer.parseInt(f[2]), "minecraft:enchanting_table").api;
        org.bukkit.enchantments.EnchantmentOffer[] offers = new org.bukkit.enchantments.EnchantmentOffer[3];
        for (int i = 0; i < 3; i++) {
            String[] o = lines[2 + i].split(" ");
            org.bukkit.enchantments.Enchantment ench = enchantment(o[1]);
            int cost = Integer.parseInt(o[0]);
            offers[i] = cost > 0 && ench != null
                    ? new org.bukkit.enchantments.EnchantmentOffer(ench, Integer.parseInt(o[2]), cost) : null;
        }
        String before = offersLine(offers);
        org.bukkit.event.enchantment.PrepareItemEnchantEvent e = new org.bukkit.event.enchantment.PrepareItemEnchantEvent(
                p.api, null, table, StackImpl.parse(lines[1]), offers, Integer.parseInt(f[3]));
        plugins.callEvent(e);
        if (e.isCancelled()) {
            return "c";
        }
        String after = offersLine(e.getOffers());
        return after.equals(before) ? "" : after;
    }

    private static String offersLine(org.bukkit.enchantments.EnchantmentOffer[] offers) {
        StringBuilder out = new StringBuilder();
        for (int i = 0; i < 3; i++) {
            org.bukkit.enchantments.EnchantmentOffer o = i < offers.length ? offers[i] : null;
            if (i > 0) out.append('\n');
            out.append(o == null ? "0 - -1" : o.getCost() + " " + o.getEnchantment().getKey() + " " + o.getEnchantmentLevel());
        }
        return out.toString();
    }

    /**
     * {@code EnchantItemEvent}: {@code x y z цена кнопка подсказка уровень}, строка предмета,
     * {@code чары уровень}… → {@code c}, пусто или {@code цена} и новые чары.
     */
    private static String enchant(PluginManager plugins, PlayerImpl p, String arg) {
        String[] lines = arg.split("\n", -1);
        String[] f = lines[0].split(" ");
        Block table = new BlockImpl(p.getWorld(), Integer.parseInt(f[0]), Integer.parseInt(f[1]),
                Integer.parseInt(f[2]), "minecraft:enchanting_table").api;
        java.util.Map<org.bukkit.enchantments.Enchantment, Integer> map = new java.util.LinkedHashMap<>();
        for (int i = 2; i < lines.length; i++) {
            String[] o = lines[i].split(" ");
            org.bukkit.enchantments.Enchantment ench = o.length == 2 ? enchantment(o[0]) : null;
            if (ench != null) map.put(ench, Integer.parseInt(o[1]));
        }
        java.util.Map<org.bukkit.enchantments.Enchantment, Integer> before = new java.util.LinkedHashMap<>(map);
        int cost = Integer.parseInt(f[3]);
        org.bukkit.event.enchantment.EnchantItemEvent e = new org.bukkit.event.enchantment.EnchantItemEvent(
                p.api, null, table, StackImpl.parse(lines[1]), cost, map, enchantment(f[5]),
                Integer.parseInt(f[6]), Integer.parseInt(f[4]));
        plugins.callEvent(e);
        // Как в CraftBukkit: пустой набор чар — тоже отмена.
        if (e.isCancelled() || e.getEnchantsToAdd().isEmpty()) {
            return "c";
        }
        if (e.getExpLevelCost() == cost && e.getEnchantsToAdd().equals(before)) {
            return "";
        }
        StringBuilder out = new StringBuilder().append(e.getExpLevelCost());
        e.getEnchantsToAdd().forEach((k, v) -> out.append('\n').append(k.getKey()).append(' ').append(v));
        return out.toString();
    }

    /** {@code x y z block ax ay az hand} → {@code c} или пусто. */
    private static String place(PluginManager plugins, PlayerImpl p, String arg) {
        String[] f = arg.split(" ");
        World w = p.getWorld();
        int x = Integer.parseInt(f[0]), y = Integer.parseInt(f[1]), z = Integer.parseInt(f[2]);
        Block placed = new BlockImpl(w, x, y, z, f[3]).api;
        Block against = new BlockImpl(w, Integer.parseInt(f[4]), Integer.parseInt(f[5]), Integer.parseInt(f[6]), null).api;
        EquipmentSlot hand = EquipmentSlot.valueOf(f[7]);
        BlockState replaced = Stub.of(BlockState.class, new StateImpl(new BlockImpl(w, x, y, z, "minecraft:air").api));
        BlockPlaceEvent e = new BlockPlaceEvent(placed, replaced, against, p.api.getInventory().getItem(hand),
                p.api, true, hand);
        plugins.callEvent(e);
        return e.isCancelled() || !e.canBuild() ? "c" : "";
    }

    /** Заменённый блок (до установки — воздух). */
    record StateImpl(Block block) {
        public Block getBlock() { return block; }
        public org.bukkit.Material getType() { return block.getType(); }
        public World getWorld() { return block.getWorld(); }
        public int getX() { return block.getX(); }
        public int getY() { return block.getY(); }
        public int getZ() { return block.getZ(); }
        public Location getLocation() { return block.getLocation(); }
    }

    /** {@code from(x y z yaw pitch) to(…)} → {@code c}, {@code t x y z yaw pitch} или пусто. */
    private static String move(PluginManager plugins, PlayerImpl p, String arg) {
        String[] f = arg.split(" ");
        Location from = location(p, f, 0), to = location(p, f, 5);
        PlayerMoveEvent e = new PlayerMoveEvent(p.api, from, to.clone());
        plugins.callEvent(e);
        if (e.isCancelled()) {
            return "c";
        }
        Location t = e.getTo();
        return t.equals(to) ? "" : "t " + t.getX() + " " + t.getY() + " " + t.getZ() + " " + t.getYaw() + " " + t.getPitch();
    }

    private static Location location(PlayerImpl p, String[] f, int i) {
        return new Location(p.getWorld(), Double.parseDouble(f[i]), Double.parseDouble(f[i + 1]),
                Double.parseDouble(f[i + 2]), Float.parseFloat(f[i + 3]), Float.parseFloat(f[i + 4]));
    }

    /** {@code ACTION x y z FACE hand} ({@code ~} — без блока) → {@code c} или пусто. */
    private static String interact(PluginManager plugins, PlayerImpl p, String arg) {
        String[] f = arg.split(" ");
        Action action = Action.valueOf(f[0]);
        EquipmentSlot hand = EquipmentSlot.valueOf(f[5]);
        ItemStack item = p.api.getInventory().getItem(hand);
        boolean air = f[1].equals("~");
        // Paper не шлёт клик в воздух пустой рукой.
        if (air && (item == null || item.isEmpty())) {
            return "";
        }
        Block block = air ? null
                : new BlockImpl(p.getWorld(), Integer.parseInt(f[1]), Integer.parseInt(f[2]), Integer.parseInt(f[3]), null).api;
        PlayerInteractEvent e = new PlayerInteractEvent(p.api, action, item == null || item.isEmpty() ? null : item,
                block, BlockFace.valueOf(f[4]), hand);
        plugins.callEvent(e);
        boolean denied = air ? e.useItemInHand() == org.bukkit.event.Event.Result.DENY : e.isCancelled();
        return denied ? "c" : "";
    }

    /**
     * {@code CAUSE ACTION [ключ усиление тики фон частицы значок [старые…]]} → {@code c} или пусто.
     * Причина COMMAND или EXPIRATION; действие ADDED, CHANGED, REMOVED, CLEARED.
     */
    private static String potion(PluginManager plugins, PlayerImpl p, String arg) {
        String[] f = arg.split(" ");
        var cause = org.bukkit.event.entity.EntityPotionEffectEvent.Cause.valueOf(f[0]);
        var action = org.bukkit.event.entity.EntityPotionEffectEvent.Action.valueOf(f[1]);
        org.bukkit.potion.PotionEffect neu = null;
        org.bukkit.potion.PotionEffect old = null;
        if (f.length >= 8) {
            var type = org.bukkit.Registry.EFFECT.get(NamespacedKey.fromString(f[2]));
            if (type != null) {
                var built = potionEffect(type, f, 3);
                if (action == org.bukkit.event.entity.EntityPotionEffectEvent.Action.REMOVED) {
                    old = built;
                } else {
                    neu = built;
                    if (action == org.bukkit.event.entity.EntityPotionEffectEvent.Action.CHANGED && f.length >= 13) {
                        old = potionEffect(type, f, 8);
                    }
                }
            }
        }
        boolean override = action == org.bukkit.event.entity.EntityPotionEffectEvent.Action.ADDED
                || action == org.bukkit.event.entity.EntityPotionEffectEvent.Action.CHANGED;
        var e = new org.bukkit.event.entity.EntityPotionEffectEvent(p.api, old, neu, null, cause, action, override);
        plugins.callEvent(e);
        return e.isCancelled() ? "c" : "";
    }

    private static org.bukkit.potion.PotionEffect potionEffect(org.bukkit.potion.PotionEffectType type, String[] f, int at) {
        return new org.bukkit.potion.PotionEffect(type, Integer.parseInt(f[at + 1]), Integer.parseInt(f[at]),
                f[at + 2].equals("1"), f[at + 3].equals("1"), f[at + 4].equals("1"));
    }

    /** {@code CAUSE amount} → {@code c} или {@code d <итоговый урон>}. */
    private static String damage(PluginManager plugins, PlayerImpl p, String arg) {
        String[] f = arg.split(" ");
        EntityDamageEvent.DamageCause cause = EntityDamageEvent.DamageCause.valueOf(f[0]);
        EntityDamageEvent e = new EntityDamageEvent(p.api, cause, source(typeKey(cause)), Double.parseDouble(f[1]));
        plugins.callEvent(e);
        return e.isCancelled() ? "c" : "d " + (float) e.getFinalDamage();
    }

    private static String typeKey(EntityDamageEvent.DamageCause cause) {
        return switch (cause) {
            case FALL -> "minecraft:fall";
            case VOID -> "minecraft:out_of_world";
            case STARVATION -> "minecraft:starve";
            case POISON, MAGIC -> "minecraft:magic";
            case WITHER -> "minecraft:wither";
            default -> "minecraft:generic";
        };
    }

    /**
     * {@code type\nmessage json\nitem count…} → {@code keepInventory\nmessage json\nitem count…}
     * (как {@code CraftEventFactory.callPlayerDeathEvent}).
     */
    private static String death(PluginManager plugins, PlayerImpl p, String arg) {
        String[] lines = arg.split("\n", -1);
        List<ItemStack> drops = new ArrayList<>();
        for (int i = 2; i < lines.length; i++) {
            ItemStack s = StackImpl.parse(lines[i]);
            if (!s.isEmpty()) drops.add(s);
        }
        Component message = lines.length > 1 && !lines[1].isEmpty()
                ? GsonComponentSerializer.gson().deserialize(lines[1]) : null;
        PlayerDeathEvent e = new PlayerDeathEvent(p.api, source(lines[0]), drops, 0, message, false);
        plugins.callEvent(e);
        StringBuilder out = new StringBuilder(e.getKeepInventory() ? "1" : "0").append('\n');
        if (e.deathMessage() != null) {
            out.append(GsonComponentSerializer.gson().serialize(e.deathMessage()));
        }
        for (ItemStack s : e.getDrops()) {
            String line = StackImpl.line(s);
            if (line != null) out.append('\n').append(line);
        }
        return out.toString();
    }

    /** {@link DamageSource} без сущностей: только тип урона. */
    static DamageSource source(String typeKey) {
        DamageType type = org.bukkit.Registry.DAMAGE_TYPE.get(NamespacedKey.fromString(typeKey));
        return Stub.of(DamageSource.class, new SourceImpl(type, null, null, null));
    }

    static DamageSource.Builder sourceBuilder(DamageType type) {
        return Stub.of(DamageSource.Builder.class, new BuilderImpl(type));
    }

    record SourceImpl(DamageType type, Entity causing, Entity direct, Location location) {
        public DamageType getDamageType() { return type; }
        public Entity getCausingEntity() { return causing; }
        public Entity getDirectEntity() { return direct; }
        public Location getDamageLocation() { return location; }
        public Location getSourceLocation() { return location != null ? location : direct != null ? direct.getLocation() : null; }
        public boolean isIndirect() { return causing != direct; }
        public float getFoodExhaustion() { return type.getExhaustion(); }
        public boolean scalesWithDifficulty() { return false; }
    }

    static final class BuilderImpl {
        private final DamageType type;
        private Entity causing, direct;
        private Location location;
        private DamageSource.Builder self;

        BuilderImpl(DamageType type) {
            this.type = type;
        }

        private DamageSource.Builder self() {
            if (self == null) self = Stub.of(DamageSource.Builder.class, this);
            return self;
        }

        public DamageSource.Builder withCausingEntity(Entity e) { causing = e; return self(); }
        public DamageSource.Builder withDirectEntity(Entity e) { direct = e; return self(); }
        public DamageSource.Builder withDamageLocation(Location l) { location = l; return self(); }
        public DamageSource build() { return Stub.of(DamageSource.class, new SourceImpl(type, causing, direct, location)); }
    }

    /** Сущность-предмет для событий выброса и подбора. */
    private static Item item(PlayerImpl p, String name, int count) {
        return Stub.of(Item.class, new ItemEntityImpl(p, StackImpl.of(name, count)));
    }

    static final class ItemEntityImpl {
        private final PlayerImpl owner;
        private final UUID id = UUID.randomUUID();
        private ItemStack stack;

        ItemEntityImpl(PlayerImpl owner, ItemStack stack) {
            this.owner = owner;
            this.stack = stack;
        }

        public ItemStack getItemStack() { return stack; }
        public void setItemStack(ItemStack s) { stack = s; }
        public UUID getUniqueId() { return id; }
        public World getWorld() { return owner.getWorld(); }
        public UUID getThrower() { return owner.id; }
        public boolean isValid() { return true; }

        @Override
        public String toString() {
            return "ChunkwakeItem{" + stack + "}";
        }
    }
}
