package net.minecraft.world.level.block.state;

import java.util.Collection;
import java.util.List;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.properties.Property;
import net.minecraft.world.level.material.FluidState;

/** NMS {@code BlockState}: блок + сетевой id + свойства для плагинов. */
public class BlockState {
    private final Block block;
    private final int id;
    private final boolean air;
    private final boolean solidRender;
    private final boolean blockEntity;
    private final FluidState fluid;

    public BlockState(Block block) {
        this(block, -1, false, true, false, FluidState.EMPTY);
    }

    public BlockState(Block block, int id, boolean air, boolean solidRender,
            boolean blockEntity, FluidState fluid) {
        this.block = block;
        this.id = id;
        this.air = air;
        this.solidRender = solidRender;
        this.blockEntity = blockEntity;
        this.fluid = fluid == null ? FluidState.EMPTY : fluid;
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

    public Comparable<?> getValue(Property<?> property) {
        List<?> values = List.copyOf(property.getPossibleValues());
        return values.isEmpty() ? null : (Comparable<?>) values.get(0);
    }

    public <T extends Comparable<T>> BlockState setValue(Property<T> property, T value) {
        return this;
    }

    public boolean hasProperty(Property<?> property) {
        return this.block.getStateDefinition().getProperties().contains(property);
    }

    public java.util.Collection<Property<?>> getProperties() {
        return this.block.getStateDefinition().getProperties();
    }

    public boolean is(Block block) {
        return this.block == block;
    }

    public String getDescriptionId() {
        return this.block.getDescriptionId();
    }

    @Override
    public String toString() {
        return this.block + "[" + this.id + "]";
    }
}
