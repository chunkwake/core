package net.minecraft.world.level.block;

/** NMS {@code BaseFireBlock}. */
public class BaseFireBlock extends Block {
    public BaseFireBlock() {
    }

    public BaseFireBlock(net.minecraft.world.level.block.state.BlockBehaviour.Properties properties) {
        super(properties);
    }

    public static boolean canBePlacedAt(net.minecraft.world.level.Level level,
            net.minecraft.core.BlockPos pos, net.minecraft.core.Direction direction) {
        return false;
    }
}
