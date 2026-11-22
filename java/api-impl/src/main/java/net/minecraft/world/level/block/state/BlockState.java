package net.minecraft.world.level.block.state;

import java.util.Collection;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.properties.Property;
import net.minecraft.world.level.material.FluidState;

/** NMS {@code BlockState}: блок + сетевой id + значения свойств. */
public class BlockState extends BlockBehaviour.BlockStateBase {
    /** {@code BlockState.CODEC}: строковый codec состояния. */
    public static final com.mojang.serialization.Codec<BlockState> CODEC =
            com.mojang.serialization.Codec.STRING.xmap(
                    text -> BlockStateParser.parse(text),
                    BlockState::toString);

    private final Block block;
    private final int id;
    private final boolean air;
    private final boolean solidRender;
    private final boolean blockEntity;
    private final FluidState fluid;
    private final Map<Property<?>, Comparable<?>> values;

    public BlockState(Block block) {
        this(block, -1, isAirName(block == null ? null : block.name()),
                true, false, FluidState.EMPTY, Map.of());
    }

    private static boolean isAirName(String name) {
        return name == null || name.equals("minecraft:air")
                || name.equals("minecraft:cave_air") || name.equals("minecraft:void_air");
    }

    public BlockState(Block block, Property<?>[] keys, Comparable<?>[] values) {
        this(block, -1, false, true, false, FluidState.EMPTY, zip(keys, values));
    }

    public BlockState(Block block, int id, boolean air, boolean solidRender,
            boolean blockEntity, FluidState fluid) {
        this(block, id, air, solidRender, blockEntity, fluid, Map.of());
    }

    public BlockState(Block block, int id, boolean air, boolean solidRender,
            boolean blockEntity, FluidState fluid, Map<Property<?>, Comparable<?>> values) {
        this.block = block;
        this.id = id;
        this.air = air;
        this.solidRender = solidRender;
        this.blockEntity = blockEntity;
        this.fluid = fluid == null ? FluidState.EMPTY : fluid;
        this.values = values == null || values.isEmpty()
                ? Map.of() : java.util.Collections.unmodifiableMap(new LinkedHashMap<>(values));
    }

    private static Map<Property<?>, Comparable<?>> zip(Property<?>[] keys, Comparable<?>[] values) {
        if (keys == null || values == null || keys.length == 0) {
            return Map.of();
        }
        Map<Property<?>, Comparable<?>> map = new LinkedHashMap<>();
        for (int i = 0; i < keys.length; i++) {
            if (keys[i] != null && i < values.length && values[i] != null) {
                map.put(keys[i], values[i]);
            }
        }
        return map;
    }

    public Block getBlock() {
        return this.block;
    }

    /** Сетевой id состояния ({@code Block.getId}); -1 для заглушек. */
    public int id() {
        return this.id;
    }

    public boolean isAir() {
        return this.air;
    }

    public boolean isSolidRender() {
        return this.solidRender;
    }

    public boolean hasBlockEntity() {
        return this.blockEntity;
    }

    public FluidState getFluidState() {
        return this.fluid;
    }

    /** Значения всех свойств этого состояния (для парсера и отладки). */
    public Map<Property<?>, Comparable<?>> getValues() {
        return this.values;
    }

    @Override
    @SuppressWarnings("unchecked")
    public <T extends Comparable<T>> T getValue(Property<T> property) {
        Comparable<?> value = this.values.get(property);
        if (value != null) {
            return (T) value;
        }
        // Свойство могло быть пересоздано (например, после /reload):
        // ищем по имени, как это делает StateHolder.
        for (Map.Entry<Property<?>, Comparable<?>> entry : this.values.entrySet()) {
            if (entry.getKey().getName().equals(property.getName())) {
                return (T) entry.getValue();
            }
        }
        List<T> possible = List.copyOf(property.getPossibleValues());
        return possible.isEmpty() ? null : possible.get(0);
    }

    @Override
    @SuppressWarnings("unchecked")
    public <T extends Comparable<T>, V extends T> BlockState setValue(Property<T> property, V value) {
        Map<Property<?>, Comparable<?>> next = new LinkedHashMap<>(this.values);
        next.put(property, value);
        BlockState state = this.block.stateFor(next);
        return state == null ? this : state;
    }

    @Override
    @SuppressWarnings("unchecked")
    public <T extends Comparable<T>> BlockState cycle(Property<T> property) {
        List<T> possible = List.copyOf(property.getPossibleValues());
        if (possible.isEmpty()) {
            return this;
        }
        T current = this.getValue(property);
        int index = possible.indexOf(current);
        return this.setValue(property, possible.get((index + 1) % possible.size()));
    }

    @Override
    @SuppressWarnings("unchecked")
    public <T extends Comparable<T>, V extends T> BlockState trySetValue(Property<T> property, V value) {
        return this.hasProperty(property) ? this.setValue(property, value) : this;
    }

    @Override
    public boolean hasProperty(Property<?> property) {
        if (property == null) {
            return false;
        }
        if (this.values.containsKey(property)) {
            return true;
        }
        for (Property<?> candidate : this.values.keySet()) {
            if (candidate.getName().equals(property.getName())) {
                return true;
            }
        }
        return false;
    }

    public net.minecraft.world.level.material.MapColor getMapColor(
            net.minecraft.world.level.BlockGetter level, net.minecraft.core.BlockPos pos) {
        return this.block.defaultMapColor();
    }

    public java.util.Collection<Property<?>> getProperties() {
        return this.block.getStateDefinition().getProperties();
    }

    public String getDescriptionId() {
        return this.block.getDescriptionId();
    }

    @Override
    public String toString() {
        if (this.values.isEmpty()) {
            return this.block.id();
        }
        StringBuilder out = new StringBuilder(this.block.id()).append('[');
        boolean first = true;
        for (Map.Entry<Property<?>, Comparable<?>> entry : this.values.entrySet()) {
            if (!first) {
                out.append(',');
            }
            out.append(entry.getKey().getName()).append('=')
                    .append(entry.getValue().toString().toLowerCase(java.util.Locale.ROOT));
            first = false;
        }
        return out.append(']').toString();
    }

    /** Парсер строкового представления (для codec и тестов). */
    static final class BlockStateParser {
        private BlockStateParser() {
        }

        static BlockState parse(String text) {
            if (text == null || text.isBlank()) {
                return net.minecraft.world.level.block.Blocks.AIR.defaultBlockState();
            }
            net.minecraft.commands.arguments.blocks.BlockStateParser.BlockResult result =
                    net.minecraft.commands.arguments.blocks.BlockStateParser.parseForBlock(
                            net.minecraft.core.registries.BuiltInRegistries.BLOCK, text, false);
            return result == null || result.blockState == null
                    ? net.minecraft.world.level.block.Blocks.AIR.defaultBlockState()
                    : result.blockState;
        }
    }
}
