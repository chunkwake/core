package net.minecraft.core;

/** NMS {@code HolderSet} (+ Named) для Orebfuscator. */
public interface HolderSet<T> {
    int size();

    T get(int index);

    final class Named<T> implements HolderSet<T> {
        private final net.minecraft.resources.Identifier key;

        public Named(net.minecraft.resources.Identifier key) {
            this.key = key;
        }

        public net.minecraft.resources.Identifier key() {
            return this.key;
        }

        @Override
        public int size() {
            return 0;
        }

        @Override
        public T get(int index) {
            throw new IndexOutOfBoundsException(index);
        }
    }
}
