package dev.chunkwake.bridge;

import io.papermc.paper.registry.RegistryAccess;
import io.papermc.paper.registry.RegistryKey;
import io.papermc.paper.registry.tag.Tag;
import io.papermc.paper.registry.tag.TagKey;
import java.util.Collection;
import java.util.Iterator;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.function.Function;
import java.util.stream.Stream;
import org.bukkit.Keyed;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.Registry;
import org.bukkit.damage.DamageEffect;
import org.bukkit.damage.DamageScaling;
import org.bukkit.damage.DamageType;
import org.bukkit.damage.DeathMessageType;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.ItemType;

/**
 * Реестры paper-api (находится через ServiceLoader). {@code Material.asItemType},
 * {@code ItemStack.of} и {@code DamageType.*} идут сюда; предметы — из данных
 * Rust-сервера, остальные реестры пока пустые.
 */
public final class Registries implements RegistryAccess {
    /** Предметы сервера: ключ → размер стопки. Заполняется до загрузки плагинов. */
    private static final Map<NamespacedKey, Integer> ITEM_STACKS = new ConcurrentHashMap<>();

    /*
     * Реестры создаются лениво в экземпляре: первый KeyedRegistry инициализирует
     * интерфейс Registry, а его <clinit> сам вызывает getRegistry — статические
     * поля в этот момент ещё были бы null.
     */
    private KeyedRegistry<ItemType> items;
    private KeyedRegistry<org.bukkit.block.BlockType> blocks;
    private KeyedRegistry<org.bukkit.inventory.MenuType> menus;

    /** Меню 26.3 (`menu`): нужны `MenuType` и `InventoryType` при инициализации. */
    private static final List<String> MENUS = List.of("generic_9x1", "generic_9x2", "generic_9x3", "generic_9x4",
            "generic_9x5", "generic_9x6", "generic_3x3", "crafter_3x3", "anvil", "beacon", "blast_furnace",
            "brewing_stand", "crafting", "enchantment", "furnace", "grindstone", "hopper", "lectern", "loom",
            "merchant", "shulker_box", "smithing", "smoker", "cartography_table", "stonecutter");

    private synchronized KeyedRegistry<org.bukkit.inventory.MenuType> menus() {
        if (menus == null) {
            menus = new KeyedRegistry<>(
                    k -> k.getNamespace().equals("minecraft") && MENUS.contains(k.getKey())
                            ? Stub.of(org.bukkit.inventory.MenuType.Typed.class, new MenuTypeImpl(k)) : null,
                    () -> MENUS.stream().map(NamespacedKey::minecraft));
        }
        return menus;
    }

    /** {@link org.bukkit.inventory.MenuType}: ключ (окна плагинов пока не открываются). */
    static final class MenuTypeImpl {
        private final NamespacedKey key;

        MenuTypeImpl(NamespacedKey key) {
            this.key = key;
        }

        public NamespacedKey getKey() { return key; }
        public net.kyori.adventure.key.Key key() { return key; }
        public Class<? extends org.bukkit.inventory.InventoryView> getInventoryViewClass() {
            return org.bukkit.inventory.InventoryView.class;
        }
        public boolean isEnabledByFeature(org.bukkit.World world) { return true; }

        @Override
        public String toString() {
            return key.toString();
        }
    }
    private KeyedRegistry<DamageType> damageTypes;
    private KeyedRegistry<org.bukkit.potion.PotionEffectType> effects;
    private KeyedRegistry<org.bukkit.enchantments.Enchantment> enchantments;

    /** Чары сервера (запрос {@code enchantments}): строки с уровнями, весом и ценой. */
    private synchronized KeyedRegistry<org.bukkit.enchantments.Enchantment> enchantments() {
        if (enchantments == null) {
            Map<NamespacedKey, String[]> all = new java.util.LinkedHashMap<>();
            for (String line : world("enchantments").split("\n")) {
                String[] f = line.trim().split(" ");
                NamespacedKey k = f.length == 10 ? NamespacedKey.fromString(f[0]) : null;
                if (k != null) {
                    all.put(k, f);
                }
            }
            enchantments = new KeyedRegistry<>(
                    k -> all.containsKey(k) ? new EnchantmentImpl(k, all.get(k)) : null,
                    () -> all.keySet().stream());
        }
        return enchantments;
    }

    /** Эффекты сервера по порядку ID (запрос {@code effects} к Rust). */
    private synchronized KeyedRegistry<org.bukkit.potion.PotionEffectType> effects() {
        if (effects == null) {
            List<NamespacedKey> keys = new java.util.ArrayList<>();
            for (String line : world("effects").split("\n")) {
                NamespacedKey k = NamespacedKey.fromString(line.trim());
                if (k != null) {
                    keys.add(k);
                }
            }
            effects = new KeyedRegistry<>(
                    k -> keys.contains(k) ? new EffectTypeImpl(k, keys.indexOf(k) + 1) : null,
                    keys::stream);
        }
        return effects;
    }

    private synchronized KeyedRegistry<ItemType> items() {
        if (items == null) {
            items = new KeyedRegistry<>(
                    k -> ITEM_STACKS.containsKey(k) ? Stub.of(ItemType.Typed.class, new ItemTypeImpl(k)) : null,
                    () -> ITEM_STACKS.keySet().stream());
        }
        return items;
    }

    /** Блоки из таблицы сервера: запрос по ключу, полный список кусками. */
    private synchronized KeyedRegistry<org.bukkit.block.BlockType> blocks() {
        if (blocks == null) {
            blocks = new KeyedRegistry<>(
                    k -> isBlock(k) ? Stub.of(org.bukkit.block.BlockType.Typed.class, new BlockTypeImpl(k)) : null,
                    () -> blockKeys().stream());
        }
        return blocks;
    }

    private static final Map<NamespacedKey, Boolean> IS_BLOCK = new ConcurrentHashMap<>();
    private static volatile List<NamespacedKey> blockKeys;

    static boolean isBlock(NamespacedKey k) {
        return IS_BLOCK.computeIfAbsent(k, key -> "1".equals(world("isblock " + key)));
    }

    private static List<NamespacedKey> blockKeys() {
        if (blockKeys == null) {
            List<NamespacedKey> out = new java.util.ArrayList<>();
            while (true) {
                String part = world("blocks " + out.size());
                if (part.isEmpty()) {
                    break;
                }
                for (String n : part.split("\n")) {
                    if (!n.isEmpty()) {
                        out.add(NamespacedKey.fromString(n));
                    }
                }
            }
            blockKeys = List.copyOf(out);
        }
        return blockKeys;
    }

    private synchronized KeyedRegistry<DamageType> damageTypes() {
        if (damageTypes == null) {
            damageTypes = new KeyedRegistry<>(k -> Stub.of(DamageType.class, new DamageTypeImpl(k)), Stream::empty);
        }
        return damageTypes;
    }

    /** Строки {@code minecraft:stone 64}. */
    static void loadItems(String list) {
        for (String line : list.split("\n")) {
            String[] f = line.trim().split(" ");
            NamespacedKey key = f.length == 2 ? NamespacedKey.fromString(f[0]) : null;
            if (key != null) {
                ITEM_STACKS.put(key, Integer.parseInt(f[1]));
            }
        }
    }

    static int maxStack(Material m) {
        return ITEM_STACKS.getOrDefault(m.getKey(), 64);
    }

    /**
     * Источник данных сервера. В сервере — запрос к Rust через мост; тесты
     * подменяют его чтением тех же ванильных отчётов (без нативной части).
     */
    static volatile Function<String, String> source = Bridge::world;

    static String world(String line) {
        return source.apply(line);
    }

    /** Строки ответа {@code registry <имя> <с какой>} целиком: ответ приходит кусками. */
    static List<String> registryLines(String name) {
        List<String> out = new java.util.ArrayList<>();
        while (true) {
            String part = world("registry " + name + " " + out.size());
            if (part.isEmpty()) {
                return out;
            }
            for (String line : part.split("\n")) {
                if (!line.isEmpty()) {
                    out.add(line);
                }
            }
        }
    }

    /**
     * Порядок инициализации классов, как при загрузке Paper: {@code GameRule}
     * раньше {@code GameRules}. Иначе плагин, первым тронувший
     * {@code GameRules.X}, ловит цикл {@code <clinit>}: {@code GameRule}
     * читает ещё пустые поля {@code GameRules} и строит мост к {@code null}.
     */
    static void bootstrap() {
        try {
            Class.forName("org.bukkit.GameRule", true, Registries.class.getClassLoader());
        } catch (Throwable t) {
            Bridge.log(2, "registries: GameRule init failed: " + t);
        }
    }

    /** Тип значений реестра — из объявления поля {@code RegistryKey<T>}. */
    private static final Map<RegistryKey<?>, Class<?>> TYPES = registryTypes();

    private static Map<RegistryKey<?>, Class<?>> registryTypes() {
        Map<RegistryKey<?>, Class<?>> out = new java.util.LinkedHashMap<>();
        for (java.lang.reflect.Field f : RegistryKey.class.getFields()) {
            if (f.getType() != RegistryKey.class
                    || !(f.getGenericType() instanceof java.lang.reflect.ParameterizedType p)) {
                continue;
            }
            java.lang.reflect.Type arg = p.getActualTypeArguments()[0];
            if (arg instanceof java.lang.reflect.ParameterizedType pa) {
                arg = pa.getRawType();
            }
            try {
                out.put((RegistryKey<?>) f.get(null), (Class<?>) arg);
            } catch (IllegalAccessException e) {
                throw new IllegalStateException(e);
            }
        }
        return out;
    }

    /** Все ключи реестров с типами значений (для тестов и {@code getRegistry(Class)}). */
    static Map<RegistryKey<?>, Class<?>> registryTypesView() {
        return java.util.Collections.unmodifiableMap(TYPES);
    }

    private final Map<RegistryKey<?>, KeyedRegistry<?>> generic = new ConcurrentHashMap<>();

    @Override
    @SuppressWarnings({"unchecked", "rawtypes"})
    public <T extends Keyed> Registry<T> getRegistry(Class<T> type) {
        for (var e : TYPES.entrySet()) {
            if (e.getValue() == type) {
                return getRegistry((RegistryKey) e.getKey());
            }
        }
        return new KeyedRegistry<>(k -> null, Stream::empty);
    }

    @Override
    @SuppressWarnings("unchecked")
    public <T extends Keyed> Registry<T> getRegistry(RegistryKey<T> key) {
        if (key == RegistryKey.ITEM) return (Registry<T>) items();
        if (key == RegistryKey.BLOCK) return (Registry<T>) blocks();
        if (key == RegistryKey.MENU) return (Registry<T>) menus();
        if (key == RegistryKey.DAMAGE_TYPE) return (Registry<T>) damageTypes();
        if (key == RegistryKey.MOB_EFFECT) return (Registry<T>) effects();
        if (key == RegistryKey.ENCHANTMENT) return (Registry<T>) enchantments();
        KeyedRegistry<?> r = generic.get(key);
        if (r == null) {
            KeyedRegistry<?> made = RegistryValues.registry(key, TYPES.get(key));
            r = generic.putIfAbsent(key, made);
            r = r != null ? r : made;
        }
        return (Registry<T>) r;
    }

    /** Реестр с ленивым созданием значений по ключу. */
    static final class KeyedRegistry<T extends Keyed> implements Registry<T> {
        private final Function<NamespacedKey, T> factory;
        private final java.util.function.Supplier<Stream<NamespacedKey>> keys;
        private final Map<NamespacedKey, T> cache = new ConcurrentHashMap<>();

        KeyedRegistry(Function<NamespacedKey, T> factory, java.util.function.Supplier<Stream<NamespacedKey>> keys) {
            this.factory = factory;
            this.keys = keys;
        }

        @Override
        public T get(NamespacedKey key) {
            T v = cache.get(key);
            if (v == null) {
                v = factory.apply(key);
                if (v != null) {
                    T prev = cache.putIfAbsent(key, v);
                    v = prev != null ? prev : v;
                }
            }
            return v;
        }

        @Override public NamespacedKey getKey(T value) { return value.getKey(); }
        @Override public boolean hasTag(TagKey<T> key) { return false; }
        @Override public Tag<T> getTag(TagKey<T> key) { throw new java.util.NoSuchElementException(key.toString()); }
        @Override public Collection<Tag<T>> getTags() { return List.of(); }
        @Override public Stream<T> stream() { return keys.get().map(this::get); }
        @Override public Stream<NamespacedKey> keyStream() { return keys.get(); }
        @Override public int size() { return (int) keys.get().count(); }
        @Override public Iterator<T> iterator() { return stream().iterator(); }
    }

    /** {@link ItemType}: ключ, материал, стопка, создание {@link ItemStack}. */
    static final class ItemTypeImpl {
        private final NamespacedKey key;

        ItemTypeImpl(NamespacedKey key) {
            this.key = key;
        }

        public NamespacedKey getKey() { return key; }
        public net.kyori.adventure.key.Key key() { return key; }
        public Material asMaterial() { return Material.matchMaterial(key.toString()); }
        public int getMaxStackSize() { return ITEM_STACKS.getOrDefault(key, 64); }
        public short getMaxDurability() { return 0; }
        public boolean hasBlockType() { return isBlock(key); }
        public org.bukkit.block.BlockType getBlockType() {
            return isBlock(key) ? Stub.of(org.bukkit.block.BlockType.Typed.class, new BlockTypeImpl(key)) : null;
        }
        public boolean isEdible() { return false; }
        public String getTranslationKey() { return "item." + key.getNamespace() + "." + key.getKey(); }
        public String translationKey() { return getTranslationKey(); }
        public ItemStack createItemStack() { return createItemStack(1); }
        public ItemStack createItemStack(int amount) { return new StackImpl(asMaterial(), amount); }

        @Override
        public String toString() {
            return key.toString();
        }
    }

    /** {@link org.bukkit.block.BlockType}: ключ, материал, предмет. */
    static final class BlockTypeImpl {
        private final NamespacedKey key;

        BlockTypeImpl(NamespacedKey key) {
            this.key = key;
        }

        public NamespacedKey getKey() { return key; }
        public net.kyori.adventure.key.Key key() { return key; }
        public Material asMaterial() { return Material.matchMaterial(key.toString()); }
        public boolean hasItemType() { return ITEM_STACKS.containsKey(key); }
        public ItemType getItemType() {
            return hasItemType() ? Stub.of(ItemType.Typed.class, new ItemTypeImpl(key)) : null;
        }
        public boolean isAir() {
            String k = key.getKey();
            return k.equals("air") || k.equals("cave_air") || k.equals("void_air");
        }
        public boolean isBlock() { return true; }
        public String getTranslationKey() { return "block." + key.getNamespace() + "." + key.getKey(); }
        public String translationKey() { return getTranslationKey(); }

        @Override
        public String toString() {
            return key.toString();
        }

        @Override
        public boolean equals(Object o) {
            return o instanceof BlockTypeImpl b && b.key.equals(key);
        }

        @Override
        public int hashCode() {
            return key.hashCode();
        }
    }

    /** {@link org.bukkit.potion.PotionEffectType}: ключ, ID (с 1, как у CraftBukkit) и категория. */
    static final class EffectTypeImpl extends org.bukkit.potion.PotionEffectType {
        private static final java.util.Set<String> HARMFUL = java.util.Set.of("slowness", "mining_fatigue",
                "instant_damage", "nausea", "blindness", "hunger", "weakness", "poison", "wither", "levitation",
                "unluck", "darkness", "infested", "oozing", "weaving", "wind_charged");
        private static final java.util.Set<String> NEUTRAL = java.util.Set.of("glowing", "bad_omen", "trial_omen",
                "raid_omen");
        private final NamespacedKey key;
        private final int id;

        EffectTypeImpl(NamespacedKey key, int id) {
            this.key = key;
            this.id = id;
        }

        @Override public NamespacedKey getKey() { return key; }
        @Override public org.bukkit.potion.PotionEffect createEffect(int duration, int amplifier) {
            return new org.bukkit.potion.PotionEffect(this, isInstant() ? 1 : duration, amplifier);
        }
        @Override public boolean isInstant() {
            return switch (key.getKey()) {
                case "instant_health", "instant_damage", "saturation" -> true;
                default -> false;
            };
        }
        @Override public org.bukkit.potion.PotionEffectTypeCategory getCategory() {
            String k = key.getKey();
            return HARMFUL.contains(k) ? org.bukkit.potion.PotionEffectTypeCategory.HARMFUL
                    : NEUTRAL.contains(k) ? org.bukkit.potion.PotionEffectTypeCategory.NEUTRAL
                    : org.bukkit.potion.PotionEffectTypeCategory.BENEFICIAL;
        }
        @Override public org.bukkit.potion.PotionEffectType.Category getEffectCategory() {
            return switch (getCategory()) {
                case HARMFUL -> Category.HARMFUL;
                case NEUTRAL -> Category.NEUTRAL;
                default -> Category.BENEFICIAL;
            };
        }
        /** Цвета ванильных `MobEffect` (в `registries.json` их нет). Неизвестный ключ — белый. */
        private static final java.util.Map<String, org.bukkit.Color> COLOR = java.util.Map.ofEntries(
                java.util.Map.entry("speed", org.bukkit.Color.fromRGB(0x7CAFC6)),
                java.util.Map.entry("slowness", org.bukkit.Color.fromRGB(0x5A6C81)),
                java.util.Map.entry("haste", org.bukkit.Color.fromRGB(0xD9C043)),
                java.util.Map.entry("mining_fatigue", org.bukkit.Color.fromRGB(0x4A4217)),
                java.util.Map.entry("strength", org.bukkit.Color.fromRGB(0x932423)),
                java.util.Map.entry("instant_health", org.bukkit.Color.fromRGB(0xF82423)),
                java.util.Map.entry("instant_damage", org.bukkit.Color.fromRGB(0x430A09)),
                java.util.Map.entry("jump_boost", org.bukkit.Color.fromRGB(0x22FF4C)),
                java.util.Map.entry("nausea", org.bukkit.Color.fromRGB(0x551D4A)),
                java.util.Map.entry("regeneration", org.bukkit.Color.fromRGB(0xCD5CAB)),
                java.util.Map.entry("resistance", org.bukkit.Color.fromRGB(0x99453A)),
                java.util.Map.entry("fire_resistance", org.bukkit.Color.fromRGB(0xE49A1A)),
                java.util.Map.entry("water_breathing", org.bukkit.Color.fromRGB(0x2E5399)),
                java.util.Map.entry("invisibility", org.bukkit.Color.fromRGB(0x7F8392)),
                java.util.Map.entry("blindness", org.bukkit.Color.fromRGB(0x1F1F1F)),
                java.util.Map.entry("night_vision", org.bukkit.Color.fromRGB(0x1F1FA1)),
                java.util.Map.entry("hunger", org.bukkit.Color.fromRGB(0x587653)),
                java.util.Map.entry("weakness", org.bukkit.Color.fromRGB(0x484D48)),
                java.util.Map.entry("poison", org.bukkit.Color.fromRGB(0x4E9331)),
                java.util.Map.entry("wither", org.bukkit.Color.fromRGB(0x352A27)),
                java.util.Map.entry("health_boost", org.bukkit.Color.fromRGB(0xF87D23)),
                java.util.Map.entry("absorption", org.bukkit.Color.fromRGB(0x2552A5)),
                java.util.Map.entry("saturation", org.bukkit.Color.fromRGB(0xF82423)),
                java.util.Map.entry("glowing", org.bukkit.Color.fromRGB(0x94A061)),
                java.util.Map.entry("levitation", org.bukkit.Color.fromRGB(0xCEFFFF)),
                java.util.Map.entry("luck", org.bukkit.Color.fromRGB(0x339900)),
                java.util.Map.entry("unluck", org.bukkit.Color.fromRGB(0xC0A44D)),
                java.util.Map.entry("slow_falling", org.bukkit.Color.fromRGB(0xFFEFD1)),
                java.util.Map.entry("conduit_power", org.bukkit.Color.fromRGB(0x1DC2D1)),
                java.util.Map.entry("dolphins_grace", org.bukkit.Color.fromRGB(0x88A3BE)),
                java.util.Map.entry("bad_omen", org.bukkit.Color.fromRGB(0x0B6138)),
                java.util.Map.entry("hero_of_the_village", org.bukkit.Color.fromRGB(0x44FF44)),
                java.util.Map.entry("darkness", org.bukkit.Color.fromRGB(0x292721)));

        @Override public org.bukkit.Color getColor() {
            return COLOR.getOrDefault(key.getKey(), org.bukkit.Color.WHITE);
        }
        @Override public double getDurationModifier() { return 1.0; }
        @Override public int getId() { return id; }
        @Override public String getName() { return key.getKey().toUpperCase(Locale.ROOT); }
        @Override public Map<org.bukkit.attribute.Attribute, org.bukkit.attribute.AttributeModifier> getEffectAttributes() {
            return Map.of();
        }
        @Override public double getAttributeModifierAmount(org.bukkit.attribute.Attribute attribute, int amplifier) {
            throw new IllegalArgumentException("chunkwake: no attribute modifiers for " + key);
        }
        @Override public String getTranslationKey() { return "effect." + key.getNamespace() + "." + key.getKey(); }
        @Override public String translationKey() { return getTranslationKey(); }

        @Override
        public String toString() {
            return key.toString();
        }
    }

    /**
     * {@link org.bukkit.enchantments.Enchantment} по данным {@code data/minecraft/enchantment}:
     * {@code имя макс вес наковальня мин_база мин_шаг макс_база макс_шаг сокровище проклятие}.
     * Предметы ({@code supported_items}) и несовместимые чары ({@code exclusive_set}) —
     * запросом {@code enchantinfo} при первой проверке.
     */
    static final class EnchantmentImpl extends org.bukkit.enchantments.Enchantment {
        private final NamespacedKey key;
        private final int max, weight, anvil, minBase, minStep, maxBase, maxStep;
        private final boolean treasure, curse;

        EnchantmentImpl(NamespacedKey key, String[] f) {
            this.key = key;
            max = Integer.parseInt(f[1]);
            weight = Integer.parseInt(f[2]);
            anvil = Integer.parseInt(f[3]);
            minBase = Integer.parseInt(f[4]);
            minStep = Integer.parseInt(f[5]);
            maxBase = Integer.parseInt(f[6]);
            maxStep = Integer.parseInt(f[7]);
            treasure = f[8].equals("1");
            curse = f[9].equals("1");
        }

        @Override public NamespacedKey getKey() { return key; }
        @Override public String getName() { return key.getKey().toUpperCase(Locale.ROOT); }
        @Override public int getMaxLevel() { return max; }
        @Override public int getStartLevel() { return 1; }
        @Override public org.bukkit.enchantments.EnchantmentTarget getItemTarget() {
            return org.bukkit.enchantments.EnchantmentTarget.BREAKABLE;
        }
        @Override public boolean isTreasure() { return treasure; }
        @Override public boolean isCursed() { return curse; }
        private java.util.Set<String> supported, exclusive;

        private synchronized void info() {
            if (supported == null) {
                String[] l = world("enchantinfo " + key).split("\n", -1);
                supported = words(l.length > 0 ? l[0] : "");
                exclusive = words(l.length > 1 ? l[1] : "");
            }
        }

        private static java.util.Set<String> words(String s) {
            return s.isBlank() ? java.util.Set.of() : java.util.Set.of(s.trim().split(" "));
        }

        /** {@code Enchantment.areCompatible}: та же или из {@code exclusive_set} одной из двух. */
        @Override public boolean conflictsWith(org.bukkit.enchantments.Enchantment other) {
            if (other == null) {
                return false;
            }
            if (other.getKey().equals(key)) {
                return true;
            }
            info();
            if (exclusive.contains(other.getKey().toString())) {
                return true;
            }
            return other instanceof EnchantmentImpl o && (o.info0().contains(key.toString()));
        }

        private java.util.Set<String> info0() {
            info();
            return exclusive;
        }

        /** {@code Enchantment.canEnchant}: предмет из {@code supported_items} (книга — всегда). */
        @Override public boolean canEnchantItem(ItemStack item) {
            if (item == null || item.isEmpty()) {
                return false;
            }
            if (item.getType() == org.bukkit.Material.BOOK || item.getType() == org.bukkit.Material.ENCHANTED_BOOK) {
                return true;
            }
            info();
            return supported.contains(item.getType().getKey().toString());
        }
        /** Как {@code Enchantment.getFullname}: проклятия красные, уровень римскими при макс > 1. */
        @Override public net.kyori.adventure.text.Component displayName(int level) {
            net.kyori.adventure.text.Component c = description().color(curse
                    ? net.kyori.adventure.text.format.NamedTextColor.RED
                    : net.kyori.adventure.text.format.NamedTextColor.GRAY);
            if (level != 1 || max != 1) {
                c = c.append(net.kyori.adventure.text.Component.space())
                        .append(net.kyori.adventure.text.Component.translatable("enchantment.level." + level));
            }
            return c;
        }
        @Override public boolean isTradeable() { return !curse; }
        @Override public boolean isDiscoverable() { return !treasure; }
        @Override public int getMinModifiedCost(int level) { return minBase + minStep * (level - 1); }
        @Override public int getMaxModifiedCost(int level) { return maxBase + maxStep * (level - 1); }
        @Override public int getAnvilCost() { return anvil; }
        @SuppressWarnings("removal")
        @Override public io.papermc.paper.enchantments.EnchantmentRarity getRarity() {
            return weight >= 10 ? io.papermc.paper.enchantments.EnchantmentRarity.COMMON
                    : weight >= 5 ? io.papermc.paper.enchantments.EnchantmentRarity.UNCOMMON
                    : weight >= 2 ? io.papermc.paper.enchantments.EnchantmentRarity.RARE
                    : io.papermc.paper.enchantments.EnchantmentRarity.VERY_RARE;
        }
        @Override public float getDamageIncrease(int level, org.bukkit.entity.EntityCategory c) { return 0; }
        @Override public float getDamageIncrease(int level, org.bukkit.entity.EntityType t) { return 0; }
        @Override public java.util.Set<org.bukkit.inventory.EquipmentSlotGroup> getActiveSlotGroups() {
            return java.util.Set.of();
        }
        @Override public net.kyori.adventure.text.Component description() {
            return net.kyori.adventure.text.Component.translatable(translationKey());
        }
        @Override public io.papermc.paper.registry.set.RegistryKeySet<ItemType> getSupportedItems() {
            return io.papermc.paper.registry.set.RegistrySet.keySet(RegistryKey.ITEM);
        }
        @Override public io.papermc.paper.registry.set.RegistryKeySet<ItemType> getPrimaryItems() {
            return null;
        }
        @Override public int getWeight() { return weight; }
        @Override public io.papermc.paper.registry.set.RegistryKeySet<org.bukkit.enchantments.Enchantment> getExclusiveWith() {
            return io.papermc.paper.registry.set.RegistrySet.keySet(RegistryKey.ENCHANTMENT);
        }
        @Override public String translationKey() { return "enchantment." + key.getNamespace() + "." + key.getKey(); }
        @Override public String getTranslationKey() { return translationKey(); }

        @Override
        public String toString() {
            return key.toString();
        }
    }

    /** {@link DamageType}: ключ и ванильные свойства по умолчанию. */
    static final class DamageTypeImpl {
        private final NamespacedKey key;

        DamageTypeImpl(NamespacedKey key) {
            this.key = key;
        }

        public NamespacedKey getKey() { return key; }
        public net.kyori.adventure.key.Key key() { return key; }
        public String getTranslationKey() {
            return switch (key.getKey()) {
                case "out_of_world" -> "outOfWorld";
                default -> key.getKey().toLowerCase(Locale.ROOT);
            };
        }
        public DamageScaling getDamageScaling() { return DamageScaling.WHEN_CAUSED_BY_LIVING_NON_PLAYER; }
        public DamageEffect getDamageEffect() { return null; }
        public DeathMessageType getDeathMessageType() {
            return key.getKey().equals("fall") ? DeathMessageType.FALL_VARIANTS : DeathMessageType.DEFAULT;
        }
        public float getExhaustion() {
            return switch (key.getKey()) {
                case "fall", "out_of_world", "starve", "generic" -> 0.0f;
                default -> 0.1f;
            };
        }

        @Override
        public String toString() {
            return key.toString();
        }
    }
}
