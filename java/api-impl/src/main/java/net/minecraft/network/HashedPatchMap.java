package net.minecraft.network;

/** NMS {@code HashedPatchMap} (CraftEngine). */
public class HashedPatchMap {
    public HashedPatchMap() {
    }

    /** NMS {@code HashedPatchMap.HashGenerator}. */
    public interface HashGenerator
            extends java.util.function.Function<net.minecraft.core.component.TypedDataComponent, Integer> {
    }

    public static HashedPatchMap create(
            net.minecraft.core.component.DataComponentPatch patch, HashGenerator generator) {
        return new HashedPatchMap();
    }

    public boolean matches(net.minecraft.core.component.DataComponentPatch patch,
            HashGenerator generator) {
        return true;
    }
}
