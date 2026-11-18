package net.minecraft.world.level.block;

/** Минимальный NMS-класс блока для рефлексии ProtocolLib. */
public class Block {
    public static final net.minecraft.core.IdMapper<Object> BLOCK_STATE_REGISTRY =
            new net.minecraft.core.IdMapper<>();

    public net.minecraft.world.level.block.state.BlockState defaultBlockState() {
        return new net.minecraft.world.level.block.state.BlockState(this);
    }
}
