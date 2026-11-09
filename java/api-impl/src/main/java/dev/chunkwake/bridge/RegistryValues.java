package dev.chunkwake.bridge;

import io.papermc.paper.registry.RegistryKey;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.function.Supplier;
import net.kyori.adventure.text.Component;
import org.bukkit.Keyed;
import org.bukkit.NamespacedKey;

/**
 * Значения реестров Paper без своей таблицы в {@link Registries}: ключи —
 * ванильные данные сервера (запрос {@code registry <имя>}), значения — по типу.
 *
 * <ul>
 *   <li>интерфейс ({@code Attribute}, {@code Sound}, {@code Biome}…) — прокси
 *       {@link Stub} с делегатом {@link KeyedValue}: ключ, {@code name()} и
 *       {@code ordinal()} как у Paper {@code OldEnumHolderable}, ключ перевода;</li>
 *   <li>enum ({@code EntityType}, {@code Particle}, {@code PotionType}) — его константа;</li>
 *   <li>абстрактный класс ({@code GameEvent}, {@code Structure}, {@code GameRule}…) —
 *       маленький подкласс здесь же.</li>
 * </ul>
 *
 * <p>Одно значение на ключ (кэш {@link Registries.KeyedRegistry}): константы
 * Bukkit сравниваются через {@code ==}.
 */
final class RegistryValues {
    private RegistryValues() {}

    /** Ленивое значение: ключи реестра читаются при первом обращении, не при старте. */
    static final class Lazy<T> implements Supplier<T> {
        private Supplier<T> init;
        private volatile T value;

        Lazy(Supplier<T> init) {
            this.init = init;
        }

        @Override
        public T get() {
            T v = value;
            if (v == null) {
                synchronized (this) {
                    if (value == null) {
                        value = init.get();
                        init = null;
                    }
                    v = value;
                }
            }
            return v;
        }
    }

    @FunctionalInterface
    interface Factory {
        Keyed make(NamespacedKey key, int ordinal);
    }

    static Registries.KeyedRegistry<Keyed> registry(RegistryKey<?> registryKey, Class<?> type) {
        String path = registryKey.key().value();
        Lazy<Map<NamespacedKey, Integer>> index = new Lazy<>(() -> {
            Map<NamespacedKey, Integer> out = new LinkedHashMap<>();
            for (String line : Registries.registryLines(path)) {
                NamespacedKey k = NamespacedKey.fromString(line.trim());
                if (k != null) {
                    out.putIfAbsent(k, out.size());
                }
            }
            return out;
        });
        Factory factory = type == null ? (k, i) -> null : factory(path, type);
        return new Registries.KeyedRegistry<>(k -> {
            Integer ordinal = index.get().get(k);
            return ordinal == null ? null : factory.make(k, ordinal);
        }, () -> index.get().keySet().stream());
    }

    /** Компоненты без значения (кодек {@code Unit}): {@code DataComponentTypes.unvalued}. */
    private static final Set<String> UNIT_COMPONENTS = Set.of("unbreakable", "intangible_projectile", "glider",
            "waxed", "creative_slot_lock");

    @SuppressWarnings({"unchecked", "rawtypes"})
    private static Factory factory(String path, Class<?> type) {
        if (type.isEnum()) {
            Lazy<Map<NamespacedKey, Keyed>> constants = new Lazy<>(() -> {
                Map<NamespacedKey, Keyed> out = new LinkedHashMap<>();
                for (Object c : type.getEnumConstants()) {
                    try {
                        out.putIfAbsent(((Keyed) c).getKey(), (Keyed) c);
                    } catch (RuntimeException e) {
                        // UNKNOWN и подобные: ключа нет, в реестре их нет и у Paper.
                    }
                }
                return out;
            });
            return (k, i) -> constants.get().get(k);
        }
        if (type == org.bukkit.entity.memory.MemoryKey.class) {
            return (k, i) -> org.bukkit.entity.memory.MemoryKey.getByKey(k);
        }
        if (type == org.bukkit.GameEvent.class) {
            return (k, i) -> new GameEventImpl(k);
        }
        if (type == org.bukkit.generator.structure.StructureType.class) {
            return (k, i) -> new StructureTypeImpl(k);
        }
        if (type == org.bukkit.generator.structure.Structure.class) {
            Lazy<Map<NamespacedKey, String[]>> info = info("#structure_type");
            return (k, i) -> new StructureImpl(k, info.get().get(k));
        }
        if (type == org.bukkit.MusicInstrument.class) {
            Lazy<Map<NamespacedKey, String[]>> info = info("#instrument");
            return (k, i) -> new InstrumentImpl(k, info.get().get(k));
        }
        if (type == org.bukkit.GameRule.class) {
            return (k, i) -> GameRules.rule(k);
        }
        if (type == io.papermc.paper.datacomponent.DataComponentType.class) {
            return (k, i) -> UNIT_COMPONENTS.contains(k.getKey())
                    ? Stub.of(io.papermc.paper.datacomponent.DataComponentType.NonValued.class, new KeyedValue(k, i, path))
                    : Stub.of(io.papermc.paper.datacomponent.DataComponentType.Valued.class, new KeyedValue(k, i, path));
        }
        if (type == org.bukkit.attribute.Attribute.class) {
            return (k, i) -> Stub.of(org.bukkit.attribute.Attribute.class, new AttributeValue(k, i, path));
        }
        if (type.isInterface()) {
            return (k, i) -> (Keyed) Stub.of((Class) type, new KeyedValue(k, i, path));
        }
        Bridge.log(2, "registry " + path + ": no values for " + type.getName());
        return (k, i) -> null;
    }

    /** Свойства записей: строки {@code ключ поле…} служебного реестра. */
    private static Lazy<Map<NamespacedKey, String[]>> info(String name) {
        return new Lazy<>(() -> {
            Map<NamespacedKey, String[]> out = new LinkedHashMap<>();
            for (String line : Registries.registryLines(name)) {
                String[] f = line.trim().split(" ");
                NamespacedKey k = NamespacedKey.fromString(f[0]);
                if (k != null) {
                    out.put(k, f);
                }
            }
            return out;
        });
    }

    /** {@code Util.makeDescriptionId}: {@code biome.minecraft.plains}. */
    static String descriptionId(String kind, NamespacedKey key) {
        return kind + "." + key.getNamespace() + "." + key.getKey().replace('/', '.');
    }

    /**
     * Делегат значений-интерфейсов: как Paper {@code OldEnumHolderable} —
     * {@code name()} это путь ключа заглавными (у чужого namespace — весь ключ),
     * {@code ordinal()} — номер в реестре.
     */
    static class KeyedValue {
        final NamespacedKey key;
        final int ordinal;
        final String registry;

        KeyedValue(NamespacedKey key, int ordinal, String registry) {
            this.key = key;
            this.ordinal = ordinal;
            this.registry = registry;
        }

        public NamespacedKey getKey() { return key; }
        public NamespacedKey getKeyOrThrow() { return key; }
        public NamespacedKey getKeyOrNull() { return key; }
        public boolean isRegistered() { return true; }
        public net.kyori.adventure.key.Key key() { return key; }
        public String name() {
            return key.getNamespace().equals(NamespacedKey.MINECRAFT) ? key.getKey().toUpperCase(Locale.ROOT)
                    : key.toString();
        }
        public int ordinal() { return ordinal; }
        public int compareTo(org.bukkit.util.OldEnum<?> other) { return ordinal - other.ordinal(); }
        public int compareTo(Object other) {
            return other instanceof org.bukkit.util.OldEnum<?> o ? compareTo(o) : 0;
        }

        /** Ключ перевода ванилы ({@code getDescriptionId}) для типов, у которых он есть. */
        public String translationKey() {
            return switch (registry) {
                case "attribute" -> "attribute.name." + key.getKey();
                case "villager_profession" -> "entity." + key.getNamespace() + ".villager." + key.getKey();
                case "worldgen/biome" -> descriptionId("biome", key);
                default -> descriptionId(registry.substring(registry.lastIndexOf('/') + 1), key);
            };
        }
        public String getTranslationKey() { return translationKey(); }

        @Override
        public String toString() {
            return key.toString();
        }
    }

    /** {@link org.bukkit.attribute.Attribute}: умолчание и «тональность» ванильного атрибута. */
    static final class AttributeValue extends KeyedValue {
        AttributeValue(NamespacedKey key, int ordinal, String registry) {
            super(key, ordinal, registry);
        }

        public double getDefaultValue() {
            String v = Registries.world("attrdefault " + key);
            return v.isEmpty() ? 0.0 : Double.parseDouble(v);
        }

        /** {@code Attributes}: отрицательные и нейтральные, остальные положительные. */
        public org.bukkit.attribute.Attribute.Sentiment getSentiment() {
            return switch (key.getKey()) {
                case "burning_time", "fall_damage_multiplier" -> org.bukkit.attribute.Attribute.Sentiment.NEGATIVE;
                case "gravity", "scale", "waypoint_transmit_range", "waypoint_receive_range" ->
                        org.bukkit.attribute.Attribute.Sentiment.NEUTRAL;
                default -> org.bukkit.attribute.Attribute.Sentiment.POSITIVE;
            };
        }
    }

    /** {@link org.bukkit.GameEvent}: радиус и частота вибрации, как в ваниле. */
    static final class GameEventImpl extends org.bukkit.GameEvent {
        /** {@code VibrationSystem.VIBRATION_FREQUENCY_FOR_EVENT}. */
        private static final Map<String, Integer> FREQUENCY = frequencies();

        private static Map<String, Integer> frequencies() {
            Map<String, Integer> m = new java.util.HashMap<>();
            Object[][] table = {
                {1, "step", "swim", "flap"},
                {2, "projectile_land", "hit_ground", "splash", "bounce"},
                {3, "item_interact_finish", "projectile_shoot", "instrument_play"},
                {4, "entity_action", "elytra_glide", "unequip"},
                {5, "entity_dismount", "equip"},
                {6, "entity_interact", "shear", "entity_mount"},
                {7, "entity_damage"},
                {8, "drink", "eat"},
                {9, "container_close", "block_close", "block_deactivate", "block_detach"},
                {10, "container_open", "block_open", "block_activate", "block_attach", "prime_fuse",
                    "note_block_play"},
                {11, "block_change"},
                {12, "block_destroy", "fluid_pickup"},
                {13, "block_place", "fluid_place"},
                {14, "entity_place", "lightning_strike", "teleport"},
                {15, "entity_die", "explode"},
            };
            for (Object[] row : table) {
                for (int i = 1; i < row.length; i++) {
                    m.put((String) row[i], (Integer) row[0]);
                }
            }
            for (int i = 1; i <= 15; i++) {
                m.put("resonate_" + i, i);
            }
            return Map.copyOf(m);
        }

        private final NamespacedKey key;

        GameEventImpl(NamespacedKey key) {
            this.key = key;
        }

        @Override public NamespacedKey getKey() { return key; }

        /** {@code GameEvent.notificationRadius}: 16, у пластинок 10, у крика скалка 32. */
        @Override public int getRange() {
            return switch (key.getKey()) {
                case "jukebox_play", "jukebox_stop_play" -> 10;
                case "shriek" -> 32;
                default -> 16;
            };
        }

        @Override public int getVibrationLevel() { return FREQUENCY.getOrDefault(key.getKey(), 0); }

        @Override
        public String toString() {
            return key.toString();
        }
    }

    /** {@link org.bukkit.generator.structure.StructureType}: только ключ. */
    static final class StructureTypeImpl extends org.bukkit.generator.structure.StructureType {
        private final NamespacedKey key;

        StructureTypeImpl(NamespacedKey key) {
            this.key = key;
        }

        @Override public NamespacedKey getKey() { return key; }

        @Override
        public String toString() {
            return key.toString();
        }
    }

    /** {@link org.bukkit.generator.structure.Structure}: ключ и тип из файла структуры. */
    static final class StructureImpl extends org.bukkit.generator.structure.Structure {
        private final NamespacedKey key;
        private final NamespacedKey type;

        StructureImpl(NamespacedKey key, String[] info) {
            this.key = key;
            this.type = info != null && info.length > 1 ? NamespacedKey.fromString(info[1]) : null;
        }

        @Override public NamespacedKey getKey() { return key; }

        @Override public org.bukkit.generator.structure.StructureType getStructureType() {
            return type == null ? null : org.bukkit.Registry.STRUCTURE_TYPE.get(type);
        }

        @Override
        public String toString() {
            return key.toString();
        }
    }

    /** {@link org.bukkit.MusicInstrument}: звук, длительность и дальность рога из данных. */
    static final class InstrumentImpl extends org.bukkit.MusicInstrument {
        private final NamespacedKey key;
        private final NamespacedKey sound;
        private final float duration, range;

        InstrumentImpl(NamespacedKey key, String[] info) {
            this.key = key;
            boolean ok = info != null && info.length > 3;
            this.sound = ok ? NamespacedKey.fromString(info[1]) : null;
            this.duration = ok ? Float.parseFloat(info[2]) : 0;
            this.range = ok ? Float.parseFloat(info[3]) : 0;
        }

        @Override public NamespacedKey getKey() { return key; }
        @Override public float getDuration() { return duration; }
        @Override public float getRange() { return range; }
        @Override public Component description() { return Component.translatable(translationKey()); }
        @Override public org.bukkit.Sound getSound() {
            return sound == null ? null : org.bukkit.Registry.SOUNDS.get(sound);
        }
        @Override public String translationKey() { return descriptionId("instrument", key); }

        @Override
        public String toString() {
            return key.toString();
        }
    }

    /** Геймрулы: тип и умолчание из запроса {@code gamerules}, значение мира — там же. */
    static final class GameRules {
        private GameRules() {}

        /** Строки {@code имя тип умолчание текущее}. */
        static Map<String, String[]> all() {
            Map<String, String[]> out = new LinkedHashMap<>();
            for (String line : Registries.world("gamerules").split("\n")) {
                String[] f = line.trim().split(" ");
                if (f.length == 4) {
                    out.put(f[0], f);
                }
            }
            return out;
        }

        static GameRuleImpl<?> rule(NamespacedKey key) {
            String[] f = all().get(key.getKey());
            if (f == null) {
                return null;
            }
            return f[1].equals("int")
                    ? new GameRuleImpl<>(key, Integer.class, Integer.parseInt(f[2]))
                    : new GameRuleImpl<>(key, Boolean.class, Boolean.parseBoolean(f[2]));
        }

        /** Текущее значение мира в типе правила; {@code null} — нет такого правила. */
        static Object value(GameRuleImpl<?> rule) {
            String[] f = all().get(rule.key.getKey());
            if (f == null) {
                return null;
            }
            Object modern = rule.modernType == Integer.class ? (Object) Integer.parseInt(f[3])
                    : (Object) Boolean.parseBoolean(f[3]);
            return rule.toLegacy(modern);
        }

        static boolean set(GameRuleImpl<?> rule, Object value) {
            Object modern = rule.toModern(value);
            return modern != null && "1".equals(Registries.world("gamerule " + rule.key.getKey() + " " + modern));
        }

        static List<String> names() {
            return List.copyOf(all().keySet());
        }
    }

    /**
     * {@link org.bukkit.GameRule} 26.x: {@code getName()} — зарегистрированное
     * имя ({@code minecraft:keep_inventory}), перевод {@code gamerule.minecraft.…}.
     * Подкласс {@link Legacy} — {@code legacyGameRuleBridge} для старых
     * констант ({@code DISABLE_ELYTRA_MOVEMENT_CHECK} и т. п.).
     */
    static class GameRuleImpl<T> extends org.bukkit.GameRule<T> {
        final NamespacedKey key;
        final Class<?> modernType;
        private final Object modernDefault;

        GameRuleImpl(NamespacedKey key, Class<?> modernType, Object modernDefault) {
            this.key = key;
            this.modernType = modernType;
            this.modernDefault = modernDefault;
        }

        @Override public NamespacedKey getKey() { return key; }
        @Override public String getName() { return key.toString(); }
        @SuppressWarnings("unchecked")
        @Override public Class<T> getType() { return (Class<T>) modernType; }
        @SuppressWarnings("unchecked")
        @Override public T getDefaultValue() { return (T) toLegacy(modernDefault); }
        @Override public String translationKey() { return descriptionId("gamerule", key); }

        Object toLegacy(Object modern) { return modern; }
        Object toModern(Object value) { return modernType.isInstance(value) ? value : null; }

        @Override
        public String toString() {
            return key.toString();
        }
    }

    /** CraftBukkit {@code LegacyGameRuleWrapper}: своё значение поверх нового правила. */
    static final class Legacy<L, M> extends GameRuleImpl<L> {
        private final Class<L> type;
        private final java.util.function.Function<L, M> toModern;
        private final java.util.function.Function<M, L> toLegacy;

        Legacy(GameRuleImpl<M> rule, java.util.function.Function<L, M> toModern,
                java.util.function.Function<M, L> toLegacy, Class<L> type) {
            super(rule.key, rule.modernType, rule.modernDefault);
            this.type = type;
            this.toModern = toModern;
            this.toLegacy = toLegacy;
        }

        @Override public Class<L> getType() { return type; }

        @SuppressWarnings("unchecked")
        @Override Object toLegacy(Object modern) { return toLegacy.apply((M) modern); }

        @SuppressWarnings("unchecked")
        @Override Object toModern(Object value) {
            return type.isInstance(value) ? toModern.apply((L) value) : null;
        }
    }
}
