package net.minecraft.core;

import java.util.stream.Stream;
import net.minecraft.resources.ResourceKey;
import net.minecraft.tags.TagKey;

/** NMS {@code TypedInstance}: типизированный экземпляр (BlockState и т.п.). */
public interface TypedInstance<T> {
    Holder<T> typeHolder();

    default Stream<TagKey<T>> tags() {
        return Stream.empty();
    }

    default boolean is(TagKey<T> tag) {
        return false;
    }

    default boolean is(HolderSet<T> set) {
        return false;
    }

    default boolean is(T value) {
        return false;
    }

    default boolean is(Holder<T> holder) {
        return false;
    }

    default boolean is(ResourceKey<T> key) {
        return false;
    }
}
