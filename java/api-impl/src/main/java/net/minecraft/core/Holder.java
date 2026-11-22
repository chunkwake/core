package net.minecraft.core;

/** NMS {@code Holder}: ссылка на значение реестра. */
public interface Holder<T> {
    /** {@code Holder.direct(T)} (CraftEngine). */
    static <T> Holder<T> direct(T value) {
        return new Reference<>(value);
    }

    /** {@code Holder.direct(T, DataComponentMap)} (CraftEngine). */
    static <T> Holder<T> direct(T value, net.minecraft.core.component.DataComponentMap components) {
        return new Reference<>(value);
    }

    default boolean is(net.minecraft.resources.Identifier id) {
        return false;
    }

    default boolean is(net.minecraft.resources.ResourceKey<T> key) {
        return false;
    }

    default boolean is(java.util.function.Predicate<net.minecraft.resources.ResourceKey<T>> predicate) {
        return false;
    }

    default boolean is(net.minecraft.tags.TagKey<T> tag) {
        return false;
    }

    default boolean is(Holder<T> holder) {
        return this == holder;
    }

    /** {@code Holder.isBound()} (CraftEngine). */
    default boolean isBound() {
        return true;
    }

    /** {@code Holder.getRegisteredName()} (CraftEngine). */
    default String getRegisteredName() {
        return "";
    }

    T value();

    /** {@code Holder.unwrapKey}: ключ реестра, если известен. */
    default java.util.Optional<net.minecraft.resources.ResourceKey<T>> unwrapKey() {
        return java.util.Optional.empty();
    }

    /** Прямая ссылка на значение. */
    final class Reference<T> implements Holder<T> {
        private final T value;
        private final net.minecraft.resources.ResourceKey<T> key;
        private java.util.Set<net.minecraft.tags.TagKey<T>> tags = java.util.Set.of();

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

        public java.util.Set<net.minecraft.tags.TagKey<T>> getTags() {
            return this.tags;
        }

        public void setTags(java.util.Set<net.minecraft.tags.TagKey<T>> tags) {
            this.tags = tags == null ? java.util.Set.of() : tags;
        }

        public void bindValue(T value) {
        }

        public void bindComponents(net.minecraft.core.component.DataComponentMap components) {
        }

        public net.minecraft.resources.ResourceKey<T> key() {
            return this.key;
        }

        public net.minecraft.resources.ResourceKey<T> getKey() {
            return this.key;
        }

        @Override
        public String toString() {
            return "Reference[" + this.value + "]";
        }
    }
}
