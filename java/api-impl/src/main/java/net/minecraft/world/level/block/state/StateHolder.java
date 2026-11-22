package net.minecraft.world.level.block.state;

import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.properties.Property;

/**
 * NMS {@code StateHolder}: владелец набора свойств состояния.
 *
 * <p>CraftEngine берёт методы {@code hasProperty}/{@code getValue}/{@code setValue}
 * именно с этого класса (BlockStateBase наследует их), поэтому сигнатуры должны
 * совпадать с Paper.
 */
public class StateHolder<O, S> {
    public boolean hasProperty(Property<?> property) {
        return false;
    }

    @SuppressWarnings("unchecked")
    public <T extends Comparable<T>> T getValue(Property<T> property) {
        return null;
    }

    @SuppressWarnings("unchecked")
    public <T extends Comparable<T>, V extends T> S setValue(Property<T> property, V value) {
        return (S) this;
    }
}
