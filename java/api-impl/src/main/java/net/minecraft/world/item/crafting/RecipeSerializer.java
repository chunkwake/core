package net.minecraft.world.item.crafting;

/** NMS {@code RecipeSerializer} (26.x): record из codec и stream codec. */
public final class RecipeSerializer<T extends Recipe> {
    private final com.mojang.serialization.MapCodec<T> codec;
    private final net.minecraft.network.codec.StreamCodec<?> streamCodec;

    public RecipeSerializer(com.mojang.serialization.MapCodec<T> codec,
            net.minecraft.network.codec.StreamCodec<?> streamCodec) {
        this.codec = codec;
        this.streamCodec = streamCodec;
    }

    public com.mojang.serialization.MapCodec<T> codec() {
        return this.codec;
    }

    public net.minecraft.network.codec.StreamCodec<?> streamCodec() {
        return this.streamCodec;
    }
}
