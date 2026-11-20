package net.minecraft.network.protocol.common.custom;

/** NMS {@code CustomPacketPayload} (Emotecraft). */
public interface CustomPacketPayload {
    Type<? extends CustomPacketPayload> type();

    /** NMS {@code CustomPacketPayload.Type}. */
    record Type<T extends CustomPacketPayload>(
            net.minecraft.resources.Identifier id, Object codec) {
    }

    /** NMS {@code CustomPacketPayload.FallbackProvider}. */
    interface FallbackProvider<T extends CustomPacketPayload> {
        Type<? extends T> type();
    }
}
