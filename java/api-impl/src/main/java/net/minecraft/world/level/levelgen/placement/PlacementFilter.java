package net.minecraft.world.level.levelgen.placement;

/** NMS {@code PlacementFilter}: фильтр placement (CraftEngine). */
public interface PlacementFilter extends PlacementModifier {
    default boolean shouldPlace(PlacementContext context,
            net.minecraft.util.RandomSource random,
            net.minecraft.core.BlockPos pos) {
        return true;
    }
}
