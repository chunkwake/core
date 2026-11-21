package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundForgetLevelChunkPacket} (позиция как long). */
public class ClientboundForgetLevelChunkPacket implements Packet {
    private long pos;

    public ClientboundForgetLevelChunkPacket() {
    }

    public ClientboundForgetLevelChunkPacket(long pos) {
        this.pos = pos;
    }

    public ClientboundForgetLevelChunkPacket(net.minecraft.world.level.ChunkPos pos) {
        this(pos == null ? 0L : pos.pack());
    }

    public long pos() {
        return this.pos;
    }
}
