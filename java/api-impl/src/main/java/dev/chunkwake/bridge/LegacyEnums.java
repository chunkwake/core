package dev.chunkwake.bridge;

import java.lang.reflect.Field;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * Legacy-имена enum'ов Bukkit (Paper runtime patches).
 *
 * <p>Paper в рантайме принимает старые имена вроде
 * {@code EntityType.valueOf("ENDER_CRYSTAL")}, хотя константа называется
 * {@code END_CRYSTAL}. Публичные paper-api jar этого не умеют. Здесь
 * добавляем алиасы в кэш {@code Class.enumConstantDirectory}, который
 * использует {@code Enum.valueOf}.
 */
public final class LegacyEnums {
    /** Legacy → современные имена (из Paper FieldRename, entity_type). */
    private static final Map<Class<?>, Map<String, String>> ALIASES = Map.of(
            org.bukkit.entity.EntityType.class, Map.ofEntries(
                    Map.entry("PIG_ZOMBIE", "ZOMBIFIED_PIGLIN"),
                    Map.entry("DROPPED_ITEM", "ITEM"),
                    Map.entry("LEASH_HITCH", "LEASH_KNOT"),
                    Map.entry("ENDER_SIGNAL", "EYE_OF_ENDER"),
                    Map.entry("POTION", "SPLASH_POTION"),
                    Map.entry("THROWN_EXP_BOTTLE", "EXPERIENCE_BOTTLE"),
                    Map.entry("PRIMED_TNT", "TNT"),
                    Map.entry("FIREWORK", "FIREWORK_ROCKET"),
                    Map.entry("MINECART_COMMAND", "COMMAND_BLOCK_MINECART"),
                    Map.entry("MINECART_CHEST", "CHEST_MINECART"),
                    Map.entry("MINECART_FURNACE", "FURNACE_MINECART"),
                    Map.entry("MINECART_TNT", "TNT_MINECART"),
                    Map.entry("MINECART_HOPPER", "HOPPER_MINECART"),
                    Map.entry("MINECART_MOB_SPAWNER", "SPAWNER_MINECART"),
                    Map.entry("MUSHROOM_COW", "MOOSHROOM"),
                    Map.entry("SNOWMAN", "SNOW_GOLEM"),
                    Map.entry("FISHING_HOOK", "FISHING_BOBBER"),
                    Map.entry("LIGHTNING", "LIGHTNING_BOLT"),
                    Map.entry("XP_ORB", "EXPERIENCE_ORB"),
                    Map.entry("EYE_OF_ENDER_SIGNAL", "EYE_OF_ENDER"),
                    Map.entry("XP_BOTTLE", "EXPERIENCE_BOTTLE"),
                    Map.entry("FIREWORKS_ROCKET", "FIREWORK_ROCKET"),
                    Map.entry("EVOCATION_FANGS", "EVOKER_FANGS"),
                    Map.entry("EVOCATION_ILLAGER", "EVOKER"),
                    Map.entry("VINDICATION_ILLAGER", "VINDICATOR"),
                    Map.entry("ILLUSION_ILLAGER", "ILLUSIONER"),
                    Map.entry("COMMANDBLOCK_MINECART", "COMMAND_BLOCK_MINECART"),
                    Map.entry("VILLAGER_GOLEM", "IRON_GOLEM"),
                    Map.entry("ENDER_CRYSTAL", "END_CRYSTAL"),
                    Map.entry("ZOMBIE_PIGMAN", "ZOMBIFIED_PIGLIN"),
                    Map.entry("BOAT", "OAK_BOAT"),
                    Map.entry("CHEST_BOAT", "OAK_CHEST_BOAT")));

    private static boolean installed;

    private LegacyEnums() {}

    /** Идемпотентно добавляет алиасы; вызывается до загрузки плагинов. */
    public static synchronized void install() {
        if (installed) {
            return;
        }
        installed = true;
        for (Map.Entry<Class<?>, Map<String, String>> entry : ALIASES.entrySet()) {
            install(entry.getKey(), entry.getValue());
        }
    }

    private static void install(Class<?> type, Map<String, String> names) {
        try {
            // Сначала наполняем кэш штатным путём: enumConstantDirectory
            // строится лениво при первом Enum.valueOf.
            Object[] constants = type.getEnumConstants();
            if (constants == null || constants.length == 0) {
                return;
            }
            @SuppressWarnings({"unchecked", "rawtypes"})
            Class<? extends Enum> enumType = (Class<? extends Enum>) type;
            Enum.valueOf(enumType, ((Enum<?>) constants[0]).name());
            Field field = Class.class.getDeclaredField("enumConstantDirectory");
            field.setAccessible(true);
            Object raw = field.get(type);
            if (!(raw instanceof Map<?, ?> existing)) {
                return;
            }
            @SuppressWarnings("unchecked")
            Map<String, Object> directory = (Map<String, Object>) existing;
            for (Map.Entry<String, String> alias : names.entrySet()) {
                Object value = directory.get(alias.getValue());
                if (value != null) {
                    directory.put(alias.getKey(), value);
                }
            }
            Bridge.log(3, "legacy enum aliases: " + type.getSimpleName() + " " + names.size());
        } catch (Throwable t) {
            Bridge.log(1, "legacy enum aliases failed for " + type.getSimpleName() + ": " + t);
        }
    }

    /** Для тестов: копия карты алиасов. */
    static Map<String, String> aliasesOf(Class<?> type) {
        return new LinkedHashMap<>(ALIASES.getOrDefault(type, Map.of()));
    }
}
