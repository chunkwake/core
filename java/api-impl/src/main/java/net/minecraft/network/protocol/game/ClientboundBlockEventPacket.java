package net.minecraft.network.protocol.game;

import net.minecraft.core.BlockPos;
import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundBlockEventPacket} (CraftEngine). */
public class ClientboundBlockEventPacket implements Packet {
    public ClientboundBlockEventPacket() {
    }

    public ClientboundBlockEventPacket(BlockPos pos, net.minecraft.world.level.block.Block block,
            int b0, int b1) {
    }
}
