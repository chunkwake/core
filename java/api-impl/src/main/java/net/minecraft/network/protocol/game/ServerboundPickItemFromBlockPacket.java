package net.minecraft.network.protocol.game;

import net.minecraft.core.BlockPos;
import net.minecraft.network.protocol.Packet;

/** NMS {@code ServerboundPickItemFromBlockPacket} (CraftEngine). */
public class ServerboundPickItemFromBlockPacket implements Packet {
    private BlockPos pos = BlockPos.ZERO;

    public ServerboundPickItemFromBlockPacket() {
    }

    public ServerboundPickItemFromBlockPacket(BlockPos pos, boolean includeData) {
        this.pos = pos == null ? BlockPos.ZERO : pos;
    }

    public BlockPos getPos() { return this.pos; }
}
