package net.minecraft.world.level;

import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.state.BlockState;

/** NMS {@code Level} (минимально, но с настоящей иерархией). */
public class Level implements LevelAccessor {
    /** Поле нужно ProtocolLib (BukkitConverters ищет CraftWorld в Level). */
    public org.bukkit.craftbukkit.CraftWorld world;

    @Override
    public int getHeight() {
        return 384;
    }

    @Override
    public BlockState getBlockState(BlockPos pos) {
        return net.minecraft.world.level.block.Blocks.AIR.defaultBlockState();
    }

    @Override
    public boolean setBlock(BlockPos pos, BlockState state, int flags) {
        return true;
    }

    public net.minecraft.core.RegistryAccess registryAccess() {
        return net.minecraft.core.RegistryAccess.EMPTY_REGISTRY_ACCESS();
    }

    public long getGameTime() {
        return 0L;
    }
}
