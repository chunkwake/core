package net.minecraft.network.codec;

/** NMS {@code ByteBufCodecs} (CraftEngine). */
public final class ByteBufCodecs {
    private ByteBufCodecs() {
    }

    /** {@code ByteBufCodecs.trackDepth(StreamCodec)} (CraftEngine). */
    @SuppressWarnings("rawtypes")
    public static StreamCodec trackDepth(StreamCodec codec) {
        return codec;
    }
}
