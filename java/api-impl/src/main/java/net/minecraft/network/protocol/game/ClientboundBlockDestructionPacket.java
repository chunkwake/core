package net.minecraft.network.protocol.game;

import net.minecraft.core.BlockPos;
import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundBlockDestructionPacket} (CraftEngine). */
public class ClientboundBlockDestructionPacket implements Packet {
    private int id;
    private BlockPos pos;
    private int progress;

    public ClientboundBlockDestructionPacket() {
    }

    public ClientboundBlockDestructionPacket(int id, BlockPos pos, int progress) {
        this.id = id;
        this.pos = pos;
        this.progress = progress;
    }
}
