package net.minecraft.world.level.block.state.properties;

import java.util.Arrays;
import java.util.List;
import java.util.Optional;

/** NMS {@code EnumProperty}. */
public final class EnumProperty<E extends Enum<E>> extends Property<E> {
    private EnumProperty(String name, Class<E> type, List<E> values) {
        super(name, type, values);
    }

    public static <E extends Enum<E>> EnumProperty<E> create(String name, Class<E> type,
            E... values) {
        return new EnumProperty<>(name, type, Arrays.asList(values));
    }

    @Override
    public Optional<E> parseValue(String value) {
        return this.getPossibleValues().stream()
                .filter(v -> v.name().equalsIgnoreCase(value))
                .findFirst();
    }
}
