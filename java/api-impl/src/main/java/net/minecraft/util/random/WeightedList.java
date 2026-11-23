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

    /** {@code WeightedList.nonEmptyCodec(Codec)} (CraftEngine). */
    public static <T> com.mojang.serialization.Codec<WeightedList<T>> nonEmptyCodec(
            com.mojang.serialization.Codec<T> codec) {
        return com.mojang.serialization.Codec.list(codec)
                .xmap(WeightedList::new, WeightedList::unwrap);
    }

    public boolean isEmpty() {
        return this.entries.isEmpty();
    }

    public List<T> unwrap() {
        return this.entries;
    }
}
