package net.minecraft.world.level.block.state;

import java.util.List;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.properties.Property;

/** NMS {@code BlockState} (минимально рабочее состояние блока). */
public class BlockState {
    private final Block block;

    public BlockState(Block block) {
        this.block = block;
    }

    public Block getBlock() {
        return this.block;
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
        return "block.minecraft.air";
    }

    @Override
    public String toString() {
        return "BlockState[" + this.block + "]";
    }
}
