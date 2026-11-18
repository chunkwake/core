package net.minecraft.network;

/**
 * Минимальный NMS {@code FriendlyByteBuf}/{@code PacketDataSerializer}.
 * ProtocolLib использует класс при выборе конструкторов пакетов.
 */
public class FriendlyByteBuf {
    private final io.netty.buffer.ByteBuf buf;

    public FriendlyByteBuf(io.netty.buffer.ByteBuf buf) {
        this.buf = buf;
    }

    public io.netty.buffer.ByteBuf source() {
        return this.buf;
    }
}
