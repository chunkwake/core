package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assumptions.assumeTrue;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import io.papermc.paper.registry.RegistryKey;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.stream.Stream;
import org.bukkit.GameRule;
import org.bukkit.NamespacedKey;
import org.bukkit.Registry;
import org.bukkit.attribute.Attribute;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;

/**
 * Все реестры Paper на ванильных данных: каждый класс с константами из
 * реестра инициализируется, ни одна константа не {@code null}.
 *
 * <p>Источник — тот же кэш ванильного генератора, что читает Rust-сервер
 * ({@code target/vanilla-cache/26.3}); без него тест пропускается.
 */
class RegistriesTest {
    private static final Path ROOT = Path.of("../../target/vanilla-cache/26.3").toAbsolutePath().normalize();
    private static final Map<String, List<String>> KEYS = new HashMap<>();

    @BeforeAll
    static void vanillaSource() {
        vanillaSourceIfPresent();
    }

    /** Ванильные данные вместо Rust-сервера (один раз); без кэша — тест пропускается. */
    static synchronized void vanillaSourceIfPresent() {
        if (!KEYS.isEmpty()) {
            Registries.source = RegistriesTest::answer;
            return;
        }
        try {
            load();
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    private static void load() throws IOException {
        assumeTrue(Files.isRegularFile(ROOT.resolve("reports/registries.json")), "no vanilla cache at " + ROOT);
        JsonObject all = JsonParser.parseString(Files.readString(ROOT.resolve("reports/registries.json")))
                .getAsJsonObject();
        for (var e : all.entrySet()) {
            JsonObject entries = e.getValue().getAsJsonObject().getAsJsonObject("entries");
            List<String> keys = new ArrayList<>(entries.keySet());
            keys.sort(Comparator.comparingInt(k -> entries.getAsJsonObject(k).get("protocol_id").getAsInt()));
            KEYS.put(e.getKey().substring("minecraft:".length()), keys);
        }
        Registries.loadItems(String.join("\n", KEYS.get("item").stream().map(k -> k + " 64").toList()));
        Registries.source = RegistriesTest::answer;
        Registries.bootstrap();
    }

    /** Ключи data-driven реестра: файлы {@code data/minecraft/<имя>/**.json}. */
    private static List<String> dataKeys(String name) {
        Path dir = ROOT.resolve("data/minecraft").resolve(name);
        if (!Files.isDirectory(dir)) {
            return List.of();
        }
        try (Stream<Path> s = Files.walk(dir)) {
            return s.filter(p -> p.toString().endsWith(".json"))
                    .map(p -> dir.relativize(p).toString().replace('\\', '/').replaceAll("\\.json$", ""))
                    .sorted().map(k -> "minecraft:" + k).toList();
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    private static List<String> keys(String name) {
        return KEYS.containsKey(name) ? KEYS.get(name) : dataKeys(name);
    }

    private static String page(List<String> lines, int from) {
        return from >= lines.size() ? "" : String.join("\n", lines.subList(from, lines.size()));
    }

    /** Ответы Rust-сервера на те же запросы (см. {@code PlayPlan::world_request}). */
    private static String answer(String line) {
        String[] f = line.split(" ");
        return switch (f[0]) {
            case "registry" -> page(switch (f[1]) {
                case "#structure_type" -> dataKeys("worldgen/structure").stream().map(k -> k + " minecraft:jigsaw")
                        .toList();
                case "#instrument" -> dataKeys("instrument").stream()
                        .map(k -> k + " minecraft:item.goat_horn.sound.0 7.0 256.0").toList();
                default -> keys(f[1]);
            }, Integer.parseInt(f[2]));
            case "effects" -> String.join("\n", keys("mob_effect"));
            case "enchantments" -> String.join("\n",
                    dataKeys("enchantment").stream().map(k -> k + " 1 10 1 1 1 1 1 0 0").toList());
            case "isblock" -> keys("block").contains(f[1]) ? "1" : "0";
            case "blocks" -> page(keys("block"), Integer.parseInt(f[1]));
            case "gamerules" -> String.join("\n", keys("game_rule").stream().map(k -> {
                String name = k.substring("minecraft:".length());
                boolean number = name.startsWith("max_") || name.startsWith("players_")
                        || name.equals("random_tick_speed") || name.equals("respawn_radius")
                        || name.equals("fire_spread_radius_around_player");
                return name + (number ? " int 3 3" : " bool true false");
            }).toList());
            default -> "";
        };
    }

    @Test
    void everyRegistryConstantResolves() throws Exception {
        List<String> missing = new ArrayList<>();
        List<Class<?>> holders = new ArrayList<>(Registries.registryTypesView().values());
        // Классы-держатели констант, у которых тип значений другой.
        holders.add(org.bukkit.GameRules.class);
        holders.add(io.papermc.paper.datacomponent.DataComponentTypes.class);
        holders.add(Registry.class);
        for (Class<?> holder : holders) {
            Class.forName(holder.getName(), true, holder.getClassLoader());
            for (Field field : holder.getFields()) {
                int m = field.getModifiers();
                if (Modifier.isStatic(m) && Modifier.isFinal(m)
                        && field.getAnnotation(Deprecated.class) == null
                        && !field.getType().isPrimitive() && field.get(null) == null) {
                    missing.add(holder.getSimpleName() + "." + field.getName());
                }
            }
        }
        assertEquals(List.of(), missing);
    }

    @Test
    void valuesAreSingletonsWithVanillaNames() {
        Attribute health = Registry.ATTRIBUTE.get(NamespacedKey.minecraft("max_health"));
        assertSame(Attribute.MAX_HEALTH, health);
        assertEquals("MAX_HEALTH", health.name());
        assertEquals("attribute.name.max_health", health.translationKey());
        assertEquals(Attribute.Sentiment.NEGATIVE, Attribute.BURNING_TIME.getSentiment());
        assertEquals(KEYS.get("attribute").indexOf("minecraft:max_health"), health.ordinal());
        assertEquals("biome.minecraft.plains", org.bukkit.block.Biome.PLAINS.translationKey());
        assertSame(org.bukkit.entity.EntityType.ZOMBIE,
                Registry.ENTITY_TYPE.get(NamespacedKey.minecraft("zombie")));
        assertEquals(12, org.bukkit.GameEvent.BLOCK_DESTROY.getVibrationLevel());
        assertEquals(32, org.bukkit.GameEvent.SHRIEK.getRange());
        assertTrue(Registry.SOUNDS.size() > 1000, "sounds: " + Registry.SOUNDS.size());
        assertNotNull(org.bukkit.generator.structure.Structure.VILLAGE_PLAINS.getStructureType());
    }

    @Test
    void gameRulesKeepTypesAndLegacyBridge() {
        GameRule<Boolean> keep = org.bukkit.GameRules.KEEP_INVENTORY;
        assertEquals("minecraft:keep_inventory", keep.getName());
        assertEquals(Boolean.class, keep.getType());
        assertEquals("gamerule.minecraft.keep_inventory", keep.translationKey());
        assertEquals(Integer.class, org.bukkit.GameRules.RANDOM_TICK_SPEED.getType());
        // Старая константа: «отключить проверку» = не «проверять» (true → false).
        @SuppressWarnings("removal")
        GameRule<Boolean> legacy = GameRule.DISABLE_ELYTRA_MOVEMENT_CHECK;
        assertEquals(Boolean.FALSE, legacy.getDefaultValue());
        assertTrue(Registries.registryTypesView().containsKey(RegistryKey.GAME_RULE));
    }
}
