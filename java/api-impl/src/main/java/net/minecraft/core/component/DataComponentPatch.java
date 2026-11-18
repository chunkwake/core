package net.minecraft.core.component;

/** NMS {@code DataComponentPatch} (минимально). */
public final class DataComponentPatch {
    public static Builder builder() {
        return new Builder();
    }

    public static final class Builder {
        public <T> Builder set(DataComponentType<T> type, T value) {
            return this;
        }

        public DataComponentPatch build() {
            return new DataComponentPatch();
        }
    }
}
