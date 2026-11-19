package net.minecraft.network.protocol.game;

import net.minecraft.core.BlockPos;
import net.minecraft.network.protocol.Packet;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;

/** NMS {@code ClientboundBlockUpdatePacket} (позиция + состояние). */
public class ClientboundBlockUpdatePacket implements Packet {
    private final BlockPos pos;
    private BlockState state;
    private int stateId;

    public ClientboundBlockUpdatePacket(BlockPos pos, BlockState state) {
        this(pos, state, Block.getId(state));
    }

    public ClientboundBlockUpdatePacket(BlockPos pos, int stateId) {
        this(pos, Block.stateById(stateId), stateId);
    }

    private ClientboundBlockUpdatePacket(BlockPos pos, BlockState state, int stateId) {
        this.pos = pos;
        this.state = state;
        this.stateId = stateId;
    }

    public BlockPos getPos() {
        return this.pos;
    }

    public BlockState getBlockState() {
        return this.state;
    }

    /** Id состояния в сети (varint); его меняет ProtocolLib-мост. */
    public int stateId() {
        return this.stateId;
    }
}
