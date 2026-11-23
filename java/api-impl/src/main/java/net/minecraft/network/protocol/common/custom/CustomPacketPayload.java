package net.minecraft.network.protocol.common.custom;

/** NMS {@code CustomPacketPayload} (Emotecraft). */
public interface CustomPacketPayload {
    Type<? extends CustomPacketPayload> type();

    /** {@code CustomPacketPayload.codec(StreamMemberEncoder, StreamDecoder)} (Pl3xMap). */
    static <B, T extends CustomPacketPayload> net.minecraft.network.codec.StreamCodec<T> codec(
            net.minecraft.network.codec.StreamMemberEncoder<B, T> encoder,
            net.minecraft.network.codec.StreamDecoder<B, T> decoder) {
        return new net.minecraft.network.codec.StreamCodec<T>() {
        };
    }

    /** {@code CustomPacketPayload.createType(String)}. */
    static <T extends CustomPacketPayload> Type<T> createType(String id) {
        return new Type<>(net.minecraft.resources.Identifier.parse(id));
    }

    /** NMS {@code CustomPacketPayload.Type}. */
    record Type<T extends CustomPacketPayload>(net.minecraft.resources.Identifier id) {
    }

    /** NMS {@code CustomPacketPayload.FallbackProvider}. */
    interface FallbackProvider<T extends CustomPacketPayload> {
        Type<? extends T> type();
    }
}
