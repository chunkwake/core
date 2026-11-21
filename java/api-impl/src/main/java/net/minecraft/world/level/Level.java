package net.minecraft.world.level;

import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.state.BlockState;

/** NMS {@code Level} (минимально, но с настоящей иерархией). */
public class Level implements LevelAccessor, net.minecraft.world.level.LevelHeightAccessor {
    /** {@code Level.updateNeighborsAt(BlockPos, Block[, Orientation])} (CraftEngine). */
    public void updateNeighborsAt(net.minecraft.core.BlockPos pos,
            net.minecraft.world.level.block.Block block) {
    }

    public void updateNeighborsAt(net.minecraft.core.BlockPos pos,
            net.minecraft.world.level.block.Block block,
            net.minecraft.world.level.redstone.Orientation orientation) {
    }

    /** Поле, которое ищет CraftEngine (getDimension). */
    private net.minecraft.resources.ResourceKey<?> dimension;

    /** Поле, которое ищет CraftEngine (LevelProxy#getRandom). */
    private net.minecraft.util.RandomSource random;

    /** {@code Level.getWorld()} (CraftEngine). */
    public org.bukkit.craftbukkit.CraftWorld getWorld() {
        return null;
    }

    /** {@code Level.dimensionType()} (CraftEngine). */
    public net.minecraft.world.level.dimension.DimensionType dimensionType() {
        return new net.minecraft.world.level.dimension.DimensionType();
    }

    /** {@code Level.isLoaded(BlockPos)} (CraftEngine). */
    public boolean isLoaded(net.minecraft.core.BlockPos pos) {
        return true;
    }


    /** Ключи измерений (squaremap проверяет NETHER/END). */
    public static final net.minecraft.resources.ResourceKey<Level> OVERWORLD =
            net.minecraft.resources.ResourceKey.entryKey(
                    net.minecraft.resources.Identifier.parse("minecraft:overworld"));
    public static final net.minecraft.resources.ResourceKey<Level> NETHER =
            net.minecraft.resources.ResourceKey.entryKey(
                    net.minecraft.resources.Identifier.parse("minecraft:the_nether"));
    public static final net.minecraft.resources.ResourceKey<Level> END =
            net.minecraft.resources.ResourceKey.entryKey(
                    net.minecraft.resources.Identifier.parse("minecraft:the_end"));
    /** Поле нужно ProtocolLib (BukkitConverters ищет CraftWorld в Level). */
    public org.bukkit.craftbukkit.CraftWorld world;

    @Override
    public int getHeight() {
        return 384;
    }

    @Override
    public int getMinY() {
        return -64;
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
