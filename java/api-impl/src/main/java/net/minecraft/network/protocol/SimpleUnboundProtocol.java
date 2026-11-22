package net.minecraft.network.protocol;

/** NMS {@code SimpleUnboundProtocol}. */
public interface SimpleUnboundProtocol<T extends net.minecraft.network.PacketListener,
        B extends io.netty.buffer.ByteBuf> extends net.minecraft.network.ProtocolInfo.DetailsProvider {
    net.minecraft.network.ProtocolInfo<T> bind(java.util.function.Function<io.netty.buffer.ByteBuf, B> allocator);
}
