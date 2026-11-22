package net.minecraft.util.context;

/** NMS {@code ContextKeySet}: набор ключей контекста. */
public class ContextKeySet {
    public static Builder builder() {
        return new Builder();
    }

    public static final class Builder {
        public Builder add(ContextKey<?> key) {
            return this;
        }

        public ContextKeySet build() {
            return new ContextKeySet();
        }
    }
}
