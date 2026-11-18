package net.minecraft.core;

/** NMS {@code Holder}: ссылка на значение реестра. */
public interface Holder<T> {
    T value();

    /** Прямая ссылка на значение. */
    final class Reference<T> implements Holder<T> {
        private final T value;

        public Reference(T value) {
            this.value = value;
        }

        @Override
        public T value() {
            return this.value;
        }

        @Override
        public String toString() {
            return "Reference[" + this.value + "]";
        }
    }
}
