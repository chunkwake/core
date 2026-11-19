package net.minecraft.core;

import java.util.List;
import java.util.stream.Stream;

/** NMS {@code HolderSet} (+ {@code Named}) для Orebfuscator и тегов. */
public interface HolderSet<T> {
    int size();

    T get(int index);

    Stream<Holder<T>> stream();

    default List<Holder<T>> contents() {
        return this.stream().toList();
    }

    final class Named<T> implements HolderSet<T> {
        private final net.minecraft.tags.TagKey<T> key;
        private final List<Holder<T>> values;

        public Named(net.minecraft.tags.TagKey<T> key, List<Holder<T>> values) {
            this.key = key;
            this.values = values == null ? List.of() : List.copyOf(values);
        }

        public net.minecraft.tags.TagKey<T> key() {
            return this.key;
        }

        public java.util.Optional<net.minecraft.tags.TagKey<T>> unwrapKey() {
            return java.util.Optional.of(this.key);
        }

        @Override
        public Stream<Holder<T>> stream() {
            return this.values.stream();
        }

        @Override
        public int size() {
            return this.values.size();
        }

        @Override
        public T get(int index) {
            return this.values.get(index).value();
        }

        @Override
        public String toString() {
            return "Named[" + this.key + ", " + this.values.size() + "]";
        }
    }
}
