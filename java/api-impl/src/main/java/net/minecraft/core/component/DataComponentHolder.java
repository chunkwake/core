package net.minecraft.core.component;

/** NMS {@code DataComponentHolder}: носитель data-компонентов. */
public interface DataComponentHolder {
    default <T> T get(net.minecraft.core.component.DataComponentType<T> type) {
        return null;
    }

    default <T> T getOrDefault(
            net.minecraft.core.component.DataComponentType<T> type, T defaultValue) {
        T value = this.get(type);
        return value == null ? defaultValue : value;
    }

    default boolean has(net.minecraft.core.component.DataComponentType<?> type) {
        return this.get(type) != null;
    }
}
