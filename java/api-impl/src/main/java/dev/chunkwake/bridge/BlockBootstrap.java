package dev.chunkwake.bridge;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.io.Reader;
import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderSet;
import net.minecraft.core.SimpleRegistry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.tags.TagKey;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BooleanProperty;
import net.minecraft.world.level.block.state.properties.IntegerProperty;
import net.minecraft.world.level.block.state.properties.Property;
import net.minecraft.world.level.block.state.properties.StringProperty;
import net.minecraft.world.level.material.FluidState;

/**
 * Наполняет NMS-реестр блоков из настоящих данных генератора:
 * {@code reports/blocks.json} (id и значения свойств состояний) и
 * {@code data/minecraft/tags/block/**.json}.
 *
 * <p>Нужно плагинам, которые читают {@code BuiltInRegistries.BLOCK}
 * напрямую (CraftEngine, Orebfuscator), а не через {@code Server.getTag}.
 */
final class BlockBootstrap {
    private BlockBootstrap() {
    }

    static void load(Path dataDir) {
        if (dataDir == null) {
            return;
        }
        Path reports = dataDir.resolve("reports").resolve("blocks.json");
        if (!Files.isRegularFile(reports)) {
            Bridge.log(1, "blocks: blocks.json not found at " + reports);
            return;
        }
        int blockCount = 0;
        int stateCount = 0;
        try (Reader reader = Files.newBufferedReader(reports)) {
            JsonObject root = JsonParser.parseReader(reader).getAsJsonObject();
            // В Blocks.<clinit> уже лежат заглушки с id 0..N. Реальные id
            // начинаются с нуля, поэтому реестр нужно очистить, иначе
            // состояние с id 0 останется заглушкой air.
            Block.BLOCK_STATE_REGISTRY.clear();
            for (Map.Entry<String, JsonElement> entry : root.entrySet()) {
                String name = entry.getKey();
                JsonObject definition = entry.getValue().getAsJsonObject();
                JsonArray stateArray = definition.getAsJsonArray("states");
                if (stateArray == null) {
                    continue;
                }
                Map<String, Property<?>> properties = parseProperties(name,
                        definition.getAsJsonObject("properties"));
                Block block = new Block(name);
                List<BlockState> list = new ArrayList<>(stateArray.size());
                BlockState defaultState = null;
                for (JsonElement element : stateArray) {
                    JsonObject stateJson = element.getAsJsonObject();
                    int id = stateJson.get("id").getAsInt();
                    Map<Property<?>, Comparable<?>> values = parseValues(stateJson, properties);
                    boolean air = isAir(name);
                    BlockState state = new BlockState(block, id, air, !air, false,
                            fluidOf(name), values);
                    Block.BLOCK_STATE_REGISTRY.addMapping(state, id);
                    list.add(state);
                    stateCount++;
                    if (defaultState == null || stateJson.has("default")) {
                        defaultState = state;
                    }
                }
                block.defineStates(list, defaultState, properties.values());
                BuiltInRegistries.BLOCK.register(Identifier.parse(name), block);
                blockCount++;
            }
            linkBlocksFields();
            int tags = loadTags(dataDir);
            Bridge.log(2, "blocks: " + blockCount + " blocks, " + stateCount + " states, "
                    + tags + " tags");
        } catch (Throwable broken) {
            Bridge.log(1, "blocks: bootstrap failed: " + broken);
        }
    }

    /** Создаёт Property-объекты по секции {@code properties} из blocks.json. */
    private static Map<String, Property<?>> parseProperties(String blockName, JsonObject json) {
        Map<String, Property<?>> out = new LinkedHashMap<>();
        if (json == null) {
            return out;
        }
        for (Map.Entry<String, JsonElement> entry : json.entrySet()) {
            String name = entry.getKey();
            JsonElement value = entry.getValue();
            if (value == null || !value.isJsonArray()) {
                continue;
            }
            JsonArray array = value.getAsJsonArray();
            List<String> strings = new ArrayList<>(array.size());
            for (JsonElement element : array) {
                strings.add(element.getAsString());
            }
            if (strings.size() == 2 && strings.contains("true") && strings.contains("false")) {
                out.put(name, BooleanProperty.create(name));
            } else if (!strings.isEmpty() && strings.stream().allMatch(BlockBootstrap::isInteger)) {
                int min = strings.stream().mapToInt(Integer::parseInt).min().orElse(0);
                int max = strings.stream().mapToInt(Integer::parseInt).max().orElse(0);
                out.put(name, IntegerProperty.create(name, min, max));
            } else {
                out.put(name, StringProperty.create(name, strings));
            }
        }
        return out;
    }

    private static Map<Property<?>, Comparable<?>> parseValues(JsonObject stateJson,
            Map<String, Property<?>> properties) {
        Map<Property<?>, Comparable<?>> values = new LinkedHashMap<>();
        JsonObject stateProperties = stateJson.getAsJsonObject("properties");
        if (stateProperties == null) {
            return values;
        }
        for (Map.Entry<String, JsonElement> entry : stateProperties.entrySet()) {
            Property<?> property = properties.get(entry.getKey());
            if (property == null) {
                continue;
            }
            Comparable<?> parsed = parseValue(property, entry.getValue().getAsString());
            if (parsed != null) {
                values.put(property, parsed);
            }
        }
        return values;
    }

    @SuppressWarnings({"unchecked", "rawtypes"})
    private static Comparable<?> parseValue(Property<?> property, String text) {
        return (Comparable<?>) ((Property) property).parseValue(text).orElse(null);
    }

    private static boolean isInteger(String value) {
        try {
            Integer.parseInt(value);
            return true;
        } catch (NumberFormatException ignored) {
            return false;
        }
    }

    /**
     * Связывает статические поля {@code Blocks} с полными блоками из реестра.
     * CraftEngine (и другие плагины) берёт {@code Blocks.NOTE_BLOCK} напрямую,
     * поэтому одного {@code BuiltInRegistries.BLOCK} недостаточно.
     */
    private static void linkBlocksFields() {
        for (Field field : Blocks.class.getDeclaredFields()) {
            if (field.getType() != Block.class || !Modifier.isStatic(field.getModifiers())) {
                continue;
            }
            String id = "minecraft:" + field.getName().toLowerCase(java.util.Locale.ROOT);
            Block real = BuiltInRegistries.BLOCK.getValue(Identifier.parse(id));
            if (real == null) {
                continue;
            }
            try {
                field.setAccessible(true);
                field.set(null, real);
            } catch (Throwable ignored) {
                // final-поле может быть защищено; реестр всё равно заполнен.
            }
        }
    }

    private static boolean isAir(String name) {
        return name.equals("minecraft:air") || name.equals("minecraft:cave_air")
                || name.equals("minecraft:void_air");
    }

    private static FluidState fluidOf(String name) {
        return name.equals("minecraft:water") || name.equals("minecraft:lava")
                ? FluidState.WATER : FluidState.EMPTY;
    }

    /** {@code data/minecraft/tags/block/**.json} → теги в реестре блоков. */
    private static int loadTags(Path dataDir) throws Exception {
        Path dir = dataDir.resolve("data").resolve("minecraft").resolve("tags")
                .resolve("block");
        if (!Files.isDirectory(dir)) {
            return 0;
        }
        Map<String, List<String>> raw = new LinkedHashMap<>();
        collect(dir, "", raw);
        int count = 0;
        for (String name : raw.keySet()) {
            Set<String> ids = resolve(raw, name, new HashSet<>());
            List<Holder<Block>> holders = new ArrayList<>(ids.size());
            for (String id : ids) {
                Block block = BuiltInRegistries.BLOCK.getValue(Identifier.parse(id));
                if (block != null) {
                    holders.add(new Holder.Reference<>(block,
                            ResourceKey.entryKey(Identifier.parse(id))));
                }
            }
            TagKey<Block> key = TagKey.create(
                    ResourceKey.createRegistryKey(Identifier.parse("block")),
                    Identifier.parse(name));
            if (BuiltInRegistries.BLOCK instanceof SimpleRegistry<?> registry) {
                @SuppressWarnings("unchecked")
                SimpleRegistry<Block> typed = (SimpleRegistry<Block>) registry;
                typed.registerTag(new HolderSet.Named<>(key, holders));
                count++;
            }
        }
        return count;
    }

    private static void collect(Path dir, String prefix, Map<String, List<String>> raw)
            throws Exception {
        try (var entries = Files.list(dir)) {
            for (Path path : entries.toList()) {
                String file = path.getFileName().toString();
                if (Files.isDirectory(path)) {
                    collect(path, prefix.isEmpty() ? file : prefix + "/" + file, raw);
                    continue;
                }
                if (!file.endsWith(".json")) {
                    continue;
                }
                String stem = file.substring(0, file.length() - 5);
                String id = "minecraft:" + (prefix.isEmpty() ? stem : prefix + "/" + stem);
                try (Reader reader = Files.newBufferedReader(path)) {
                    JsonObject json = JsonParser.parseReader(reader).getAsJsonObject();
                    JsonArray values = json.getAsJsonArray("values");
                    List<String> list = new ArrayList<>();
                    if (values != null) {
                        for (JsonElement value : values) {
                            if (value.isJsonPrimitive()) {
                                list.add(value.getAsString());
                            }
                        }
                    }
                    raw.put(id, list);
                }
            }
        }
    }

    private static Set<String> resolve(Map<String, List<String>> raw, String name,
            Set<String> visiting) {
        if (!visiting.add(name)) {
            return Set.of();
        }
        List<String> values = raw.get(name);
        Set<String> out = new LinkedHashSet<>();
        if (values != null) {
            for (String value : values) {
                if (value.startsWith("#")) {
                    String nested = value.substring(1);
                    if (!nested.contains(":")) {
                        nested = "minecraft:" + nested;
                    }
                    out.addAll(resolve(raw, nested, visiting));
                } else {
                    out.add(value.contains(":") ? value : "minecraft:" + value);
                }
            }
        }
        visiting.remove(name);
        return out;
    }
}
