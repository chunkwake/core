package net.minecraft.commands.arguments.blocks;

/** NMS {@code BlockStateParser} (CraftEngine). */
public final class BlockStateParser {
    private BlockStateParser() {
    }

    /** NMS {@code BlockStateParser.BlockResult}. */
    public static final class BlockResult {
        public net.minecraft.world.level.block.state.BlockState blockState;

        public BlockResult() {
        }

        public BlockResult(net.minecraft.world.level.block.state.BlockState blockState) {
            this.blockState = blockState;
        }
    }

    public static BlockResult parseForBlock(
            net.minecraft.core.HolderLookup<net.minecraft.world.level.block.Block> lookup,
            String input, boolean allowNbt) {
        return null;
    }
}
