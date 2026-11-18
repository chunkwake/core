package net.minecraft.network;

import io.netty.buffer.ByteBuf;
import io.netty.buffer.WrappedByteBuf;

/**
 * NMS {@code FriendlyByteBuf}/{@code PacketDataSerializer}: реальный
 * {@link ByteBuf}, чтобы плагины (Axiom) могли передавать его в методы,
 * ожидающие {@code io.netty.buffer.ByteBuf}.
 */
public class FriendlyByteBuf extends WrappedByteBuf {
    public FriendlyByteBuf(ByteBuf buf) {
        super(buf);
    }

    public ByteBuf source() {
        return this.buf;
    }
}
