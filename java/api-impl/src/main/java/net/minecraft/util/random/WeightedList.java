package net.minecraft.util.random;

import java.util.List;

/** NMS {@code WeightedList}: стаб для CE state providers. */
public class WeightedList<T> {
    private final List<T> entries;

    public WeightedList(List<T> entries) {
        this.entries = entries;
    }

    public static <T> WeightedList<T> of(List<T> entries) {
        return new WeightedList<>(entries);
    }

    public boolean isEmpty() {
        return this.entries.isEmpty();
    }

    public List<T> unwrap() {
        return this.entries;
    }
}
