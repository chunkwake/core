package net.minecraft.network;

/** NMS {@code HashedStack}. */
public interface HashedStack {
    HashedStack EMPTY = new HashedStack() {
        @Override
        public boolean matches(net.minecraft.world.item.ItemStack stack,
                HashedPatchMap.HashGenerator generator) {
            return true;
        }
    };

    Object STREAM_CODEC = null;

    boolean matches(net.minecraft.world.item.ItemStack stack, HashedPatchMap.HashGenerator generator);

    static HashedStack create(net.minecraft.world.item.ItemStack stack,
            HashedPatchMap.HashGenerator generator) {
        return EMPTY;
    }
}
