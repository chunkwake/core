package net.minecraft.world.level.block.state.pattern;

/** NMS-заглушка {@code BlockInWorld} (CommandAPI проверяет наличие). */
public class BlockInWorld {
    private final net.minecraft.world.level.LevelReader level;
    private final net.minecraft.core.BlockPos pos;
    private net.minecraft.world.level.block.state.BlockState state;

    public BlockInWorld() {
        this(null, net.minecraft.core.BlockPos.ZERO, false);
    }

    public BlockInWorld(net.minecraft.world.level.LevelReader level,
            net.minecraft.core.BlockPos pos, boolean loadChunks) {
        this.level = level;
        this.pos = pos;
    }

    public net.minecraft.world.level.block.state.BlockState getState() {
        return this.state;
    }

    public void setState(net.minecraft.world.level.block.state.BlockState state) {
        this.state = state;
    }
}
