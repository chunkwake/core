package net.minecraft.core.component;

/** NMS {@code DataComponentHolder}: держатель data-компонентов. */
public interface DataComponentHolder extends DataComponentGetter {
    /** {@code DataComponentHolder.getComponents()} (CraftEngine). */
    default net.minecraft.core.component.DataComponentMap getComponents() {
        return new net.minecraft.core.component.PatchedDataComponentMap(null);
    }

    @Override
    default <T> T get(DataComponentType<? extends T> type) {
        return null;
    }

    @Override
    default boolean has(DataComponentType<?> type) {
        return this.get(type) != null;
    }
}
