package net.minecraft.network.protocol.common.custom;

import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import net.minecraft.resources.Identifier;

/** NMS {@code DiscardedPayload}: произвольные байты под заданным id. */
public record DiscardedPayload(Identifier id, byte[] data) implements CustomPacketPayload {
    public DiscardedPayload(Identifier id, ByteBuf buf) {
        this(id, copy(buf));
    }

    private static byte[] copy(ByteBuf buf) {
        byte[] bytes = new byte[buf.readableBytes()];
        buf.getBytes(buf.readerIndex(), bytes);
        return bytes;
    }

    public ByteBuf dataBuffer() {
        return Unpooled.wrappedBuffer(this.data);
    }

    @Override
    public Type<? extends CustomPacketPayload> type() {
        return new Type<>(this.id);
    }
}
