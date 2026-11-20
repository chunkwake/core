package net.minecraft.world.item.context;

/** NMS {@code BlockPlaceContext}: контекст установки блока (CraftEngine). */
public class BlockPlaceContext {
    private net.minecraft.core.BlockPos clickedPos = net.minecraft.core.BlockPos.ZERO;
    private net.minecraft.core.Direction clickedFace = net.minecraft.core.Direction.UP;
    private net.minecraft.world.item.ItemStack itemInHand = net.minecraft.world.item.ItemStack.EMPTY;
    private net.minecraft.world.level.Level level;

    public BlockPlaceContext() {
    }

    public BlockPlaceContext(net.minecraft.world.level.Level level,
            net.minecraft.world.entity.player.Player player,
            net.minecraft.world.InteractionHand hand,
            net.minecraft.world.item.ItemStack stack,
            net.minecraft.world.phys.BlockHitResult hit) {
        this.level = level;
        this.itemInHand = stack;
    }

    public net.minecraft.core.Direction getNearestLookingDirection() {
        return net.minecraft.core.Direction.UP;
    }

    public net.minecraft.core.Direction getNearestLookingVerticalDirection() {
        return net.minecraft.core.Direction.UP;
    }

    public net.minecraft.core.Direction[] getNearestLookingDirections() {
        return new net.minecraft.core.Direction[] {net.minecraft.core.Direction.UP};
    }

    public net.minecraft.core.BlockPos getClickedPos() {
        return this.clickedPos;
    }

    public net.minecraft.core.Direction getClickedFace() {
        return this.clickedFace;
    }

    public net.minecraft.world.item.ItemStack getItemInHand() {
        return this.itemInHand;
    }

    public net.minecraft.world.level.Level getLevel() {
        return this.level;
    }
}
