package net.minecraft.core;

/** NMS {@code Holder}: ссылка на значение реестра. */
public interface Holder<T> {
    T value();

    /** {@code Holder.unwrapKey}: ключ реестра, если известен. */
    default java.util.Optional<net.minecraft.resources.ResourceKey<T>> unwrapKey() {
        return java.util.Optional.empty();
    }

    /** Прямая ссылка на значение. */
    final class Reference<T> implements Holder<T> {
        private final T value;
        private final net.minecraft.resources.ResourceKey<T> key;

        public Reference(T value) {
            this(value, null);
        }

        public Reference(T value, net.minecraft.resources.ResourceKey<T> key) {
            this.value = value;
            this.key = key;
        }

        @Override
        public T value() {
            return this.value;
        }

        @Override
        public java.util.Optional<net.minecraft.resources.ResourceKey<T>> unwrapKey() {
            return java.util.Optional.ofNullable(this.key);
        }

        @Override
        public String toString() {
            return "Reference[" + this.value + "]";
        }
    }
}
