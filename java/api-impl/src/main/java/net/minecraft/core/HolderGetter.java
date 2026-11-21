package net.minecraft.core;

import java.util.Optional;
import net.minecraft.resources.ResourceKey;
import net.minecraft.tags.TagKey;

/** NMS {@code HolderGetter} (CraftEngine). */
public interface HolderGetter<T> extends HolderOwner<T> {
    Optional<Holder.Reference<T>> get(ResourceKey<T> key);

    default Holder.Reference<T> getOrThrow(ResourceKey<T> key) {
        return this.get(key).orElse(null);
    }

    Optional<HolderSet.Named<T>> get(TagKey<T> tag);

    default HolderSet.Named<T> getOrThrow(TagKey<T> tag) {
        return this.get(tag).orElse(null);
    }

    default Optional<Holder<T>> getRandomElementOf(TagKey<T> tag,
            net.minecraft.util.RandomSource random) {
        return Optional.empty();
    }

    /** NMS {@code HolderGetter.Provider} (CraftEngine). */
    interface Provider {
        <T> HolderGetter<T> lookupOrThrow(
                ResourceKey<? extends Registry<? extends T>> key);
    }
}
