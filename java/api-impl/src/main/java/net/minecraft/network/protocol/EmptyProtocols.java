package net.minecraft.network.protocol;

import io.netty.buffer.ByteBuf;
import java.util.function.Function;
import net.minecraft.network.ConnectionProtocol;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.network.PacketListener;
import net.minecraft.network.ProtocolInfo;

/**
 * Пустые protocol-шаблоны для NMS-заглушек.
 *
 * <p>CraftEngine строит карту id пакетов из {@code *Protocols.*_TEMPLATE}.
 * Шаблонов с реальными пакетами у Chunkwake нет, но объекты должны быть
 * не-null и корректно отдавать {@code Details} — иначе CE падает ещё до
 * собственной инициализации.
 */
public final class EmptyProtocols {
    private EmptyProtocols() {
    }

    public static <T extends PacketListener, B extends ByteBuf> SimpleUnboundProtocol<T, B> simple(
            ConnectionProtocol id, PacketFlow flow) {
        return new SimpleUnboundProtocol<T, B>() {
            @Override
            public ProtocolInfo<T> bind(Function<ByteBuf, B> allocator) {
                return null;
            }

            @Override
            public ProtocolInfo.Details details() {
                return new EmptyDetails(id, flow);
            }
        };
    }

    public static <T extends PacketListener, B extends ByteBuf, C> UnboundProtocol<T, B, C> unbound(
            ConnectionProtocol id, PacketFlow flow) {
        return new UnboundProtocol<>() {
            @Override
            public ProtocolInfo<T> bind(Function<ByteBuf, B> allocator, C context) {
                return null;
            }

            @Override
            public ProtocolInfo.Details details() {
                return new EmptyDetails(id, flow);
            }
        };
    }

    private record EmptyDetails(ConnectionProtocol id, PacketFlow flow)
            implements ProtocolInfo.Details {
        @Override
        public void listPackets(PacketVisitor visitor) {
        }
    }
}
