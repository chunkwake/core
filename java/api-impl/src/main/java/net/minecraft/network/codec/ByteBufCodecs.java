package net.minecraft.network.codec;

/** NMS {@code ByteBufCodecs} (CraftEngine). */
public final class ByteBufCodecs {
    private ByteBufCodecs() {
    }

    /** {@code ByteBufCodecs.trackDepth(StreamCodec)} (CraftEngine). */
    public static Object trackDepth(Object codec) {
        return codec;
    }
}
