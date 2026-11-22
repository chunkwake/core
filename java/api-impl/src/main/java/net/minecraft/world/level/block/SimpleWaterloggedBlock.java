package net.minecraft.world.level.block;

/** NMS {@code SimpleWaterloggedBlock}: блок с waterlogged-состоянием. */
public interface SimpleWaterloggedBlock {
    default boolean canPlaceLiquid(net.minecraft.world.entity.LivingEntity entity,
            net.minecraft.world.level.BlockGetter level, net.minecraft.core.BlockPos pos,
            net.minecraft.world.level.block.state.BlockState state,
            net.minecraft.world.level.material.Fluid fluid) {
        return false;
    }

    default boolean placeLiquid(net.minecraft.world.level.LevelAccessor level,
            net.minecraft.core.BlockPos pos, net.minecraft.world.level.block.state.BlockState state,
            net.minecraft.world.level.material.FluidState fluidState) {
        return false;
    }

    default net.minecraft.world.item.ItemStack pickupBlock(
            net.minecraft.world.entity.LivingEntity entity,
            net.minecraft.world.level.LevelAccessor level, net.minecraft.core.BlockPos pos,
            net.minecraft.world.level.block.state.BlockState state) {
        return net.minecraft.world.item.ItemStack.EMPTY;
    }

    default java.util.Optional<net.minecraft.sounds.SoundEvent> getPickupSound() {
        return java.util.Optional.empty();
    }
}
