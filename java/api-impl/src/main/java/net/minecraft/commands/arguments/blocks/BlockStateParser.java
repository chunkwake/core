package net.minecraft.commands.arguments.blocks;

import java.util.LinkedHashMap;
import java.util.Map;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.Property;

/**
 * NMS {@code BlockStateParser} (CraftEngine).
 *
 * <p>Разбирает каноническую строку {@code minecraft:redstone_torch[lit=false]}.
 * Возвращает именно то состояние, которое перечислил
 * {@code reports/blocks.json}, — с теми же id и значениями свойств.
 */
public final class BlockStateParser {
    private BlockStateParser() {
    }

    /** NMS {@code BlockStateParser.BlockResult}. */
    public static final class BlockResult {
        public BlockState blockState;

        public BlockResult() {
        }

        public BlockResult(BlockState blockState) {
            this.blockState = blockState;
        }
    }

    public static BlockResult parseForBlock(
            HolderLookup<Block> lookup,
            String input, boolean allowNbt) {
        if (input == null) {
            return null;
        }
        String text = input.trim();
        if (text.isEmpty()) {
            return null;
        }
        // NBT-суффикс после состояния: redstone_torch[lit=false]{...}
        int nbt = text.indexOf('{');
        if (nbt >= 0) {
            text = text.substring(0, nbt);
        }
        String idText = text;
        String propertiesText = null;
        int bracket = text.indexOf('[');
        if (bracket >= 0) {
            idText = text.substring(0, bracket);
            int close = text.lastIndexOf(']');
            if (close < bracket) {
                return null;
            }
            propertiesText = text.substring(bracket + 1, close);
        }
        Identifier id = Identifier.tryParse(idText.trim());
        if (id == null) {
            return null;
        }
        Block block = block(lookup, id);
        if (block == null) {
            return null;
        }
        BlockState state = block.defaultBlockState();
        if (propertiesText == null || propertiesText.isBlank()) {
            return new BlockResult(state);
        }
        Map<String, String> requested = new LinkedHashMap<>();
        for (String pair : propertiesText.split(",")) {
            int equals = pair.indexOf('=');
            if (equals <= 0 || equals == pair.length() - 1) {
                return null;
            }
            requested.put(pair.substring(0, equals).trim(), pair.substring(equals + 1).trim());
        }
        for (Map.Entry<String, String> entry : requested.entrySet()) {
            Property<?> property = property(block, entry.getKey());
            if (property == null) {
                return null;
            }
            Comparable<?> value = parseValue(property, entry.getValue());
            if (value == null) {
                return null;
            }
            state = apply(state, property, value);
            if (state == null) {
                return null;
            }
        }
        return new BlockResult(state);
    }

    private static Block block(HolderLookup<Block> lookup, Identifier id) {
        if (lookup != null) {
            try {
                var holder = lookup.get(ResourceKey.entryKey(id));
                if (holder != null && holder.isPresent()) {
                    return holder.get().value();
                }
            } catch (Throwable ignored) {
                // переходим к глобальному реестру
            }
        }
        return BuiltInRegistries.BLOCK.getValue(id);
    }

    private static Property<?> property(Block block, String name) {
        for (Property<?> property : block.getStateDefinition().getProperties()) {
            if (property.getName().equals(name)) {
                return property;
            }
        }
        return block.getStateDefinition().getProperty(name);
    }

    @SuppressWarnings({"unchecked", "rawtypes"})
    private static Comparable<?> parseValue(Property<?> property, String text) {
        return (Comparable<?>) ((Property) property).parseValue(text).orElse(null);
    }

    @SuppressWarnings({"unchecked", "rawtypes"})
    private static BlockState apply(BlockState state, Property<?> property, Comparable<?> value) {
        return state.setValue((Property) property, (Comparable) value);
    }
}
