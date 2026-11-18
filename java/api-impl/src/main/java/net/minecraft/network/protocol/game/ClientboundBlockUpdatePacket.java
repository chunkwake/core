package net.minecraft.network.protocol.game;

import net.minecraft.core.BlockPos;
import net.minecraft.network.protocol.Packet;
import net.minecraft.world.level.block.state.BlockState;

/** NMS {@code ClientboundBlockUpdatePacket}. */
public class ClientboundBlockUpdatePacket implements Packet {
    private final BlockPos pos;
    private final BlockState state;

    public ClientboundBlockUpdatePacket(BlockPos pos, BlockState state) {
        this.pos = pos;
        this.state = state;
    }

    public BlockPos getPos() {
        return this.pos;
    }

    public BlockState getBlockState() {
        return this.state;
    }
}
