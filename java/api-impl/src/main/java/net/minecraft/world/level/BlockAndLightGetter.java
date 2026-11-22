package net.minecraft.world.level;

import net.minecraft.core.BlockPos;

/** NMS {@code BlockAndLightGetter}: блоки + освещение. */
public interface BlockAndLightGetter extends BlockGetter {
    net.minecraft.world.level.lighting.LevelLightEngine getLightEngine();

    default int getBrightness(LightLayer layer, BlockPos pos) {
        return 0;
    }

    default int getRawBrightness(BlockPos pos, int amount) {
        return 0;
    }

    default boolean canSeeSky(BlockPos pos) {
        return false;
    }
}
