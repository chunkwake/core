package net.minecraft.core;

import net.minecraft.resources.ResourceKey;

/** Заглушка NMS {@code HolderLookup}. */
public interface HolderLookup<T> extends HolderGetter<T> {
    @Override
    default java.util.Optional<Holder.Reference<T>> get(
            ResourceKey<T> key) {
        return java.util.Optional.empty();
    }

    @Override
    default java.util.Optional<HolderSet.Named<T>> get(
            net.minecraft.tags.TagKey<T> tag) {
        return java.util.Optional.empty();
    }

    interface Provider extends HolderGetter.Provider {
        @Override
        default <T> HolderGetter<T> lookupOrThrow(
                ResourceKey<? extends Registry<? extends T>> key) {
            return null;
        }

        /** {@code HolderLookup.Provider.createSerializationContext} (nightcore). */
        default <T> net.minecraft.resources.RegistryOps<T> createSerializationContext(
                com.mojang.serialization.DynamicOps<T> ops) {
            return net.minecraft.resources.RegistryOps.create(
                    ops, RegistryAccess.Frozen.empty());
        }
    }

    interface RegistryLookup<T> extends HolderLookup<T> {
        ResourceKey<? extends Registry<? extends T>> key();

        com.mojang.serialization.Lifecycle registryLifecycle();

        default RegistryLookup<T> filterFeatures(net.minecraft.world.flag.FeatureFlagSet flags) {
            return this;
        }

        default RegistryLookup<T> filterElements(java.util.function.Predicate<T> predicate) {
            return this;
        }

        default java.util.stream.Stream<ResourceKey<T>> listElementIds() {
            return java.util.stream.Stream.empty();
        }

        default java.util.stream.Stream<Holder.Reference<T>> listElements() {
            return java.util.stream.Stream.empty();
        }

        default java.util.Optional<Holder.Reference<T>> get(
                ResourceKey<T> key) {
            return java.util.Optional.empty();
        }
    }
}
