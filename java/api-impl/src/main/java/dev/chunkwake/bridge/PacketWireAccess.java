package dev.chunkwake.bridge;

import java.util.UUID;
import net.minecraft.network.protocol.Packet;

/** Публичный фасад NMS-моста для {@code net.minecraft.network.Connection}. */
public final class PacketWireAccess {
    private PacketWireAccess() {
    }

    public static byte[] encode(Packet packet) {
        return PacketWire.encode(packet);
    }

    public static boolean send(UUID owner, byte[] wire) {
        return Bridge.sendPacket(owner, wire);
    }
}
