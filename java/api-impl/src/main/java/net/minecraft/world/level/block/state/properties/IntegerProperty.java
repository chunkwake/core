package net.minecraft.world.level.block.state.properties;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

/** NMS {@code IntegerProperty}. */
public final class IntegerProperty extends Property<Integer> {
    private final int min;
    private final int max;

    private IntegerProperty(String name, int min, int max) {
        super(name, Integer.class, range(min, max));
        this.min = min;
        this.max = max;
    }

    private static List<Integer> range(int min, int max) {
        List<Integer> values = new ArrayList<>();
        for (int i = min; i <= max; i++) {
            values.add(i);
        }
        return values;
    }

    public static IntegerProperty create(String name, int min, int max) {
        return new IntegerProperty(name, min, max);
    }

    public int getMin() {
        return this.min;
    }

    public int getMax() {
        return this.max;
    }

    @Override
    public Optional<Integer> parseValue(String value) {
        try {
            return this.getValue(String.valueOf(Integer.parseInt(value)));
        } catch (NumberFormatException e) {
            return Optional.empty();
        }
    }
}
