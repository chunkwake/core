package net.minecraft.core.component;

/** NMS {@code DataComponentGetter} (CraftEngine). */
public interface DataComponentGetter {
    <T> T get(DataComponentType<? extends T> type);

    default <T> T getOrDefault(DataComponentType<? extends T> type, T fallback) {
        T value = this.get(type);
        return value == null ? fallback : value;
    }

    default boolean has(DataComponentType<?> type) {
        return this.get(type) != null;
    }
}
