package net.minecraft.world.level;

/**
 * NMS {@code LevelReader}. Методы настоящего интерфейса объявлены default:
 * {@code Level} должен компилироваться без их реализации, а
 * {@code ServerLevel} переопределяет их реальными значениями.
 */
public interface LevelReader extends BlockGetter {
    int getHeight();

    default boolean isEmptyBlock(net.minecraft.core.BlockPos pos) {
        return this.getBlockState(pos).is(net.minecraft.world.level.block.Blocks.AIR);
    }

    default net.minecraft.world.level.chunk.ChunkAccess getChunk(int x, int z,
            net.minecraft.world.level.chunk.status.ChunkStatus status, boolean load) {
        return null;
    }

    default boolean hasChunk(int x, int z) {
        return false;
    }

    default int getHeight(net.minecraft.world.level.levelgen.Heightmap.Types type, int x, int z) {
        return 0;
    }

    default int getSkyDarken() {
        return 0;
    }

    /** {@code LevelReader.getNoiseBiome(int,int,int)} (CraftEngine). */
    default net.minecraft.core.Holder<net.minecraft.world.level.biome.Biome> getNoiseBiome(
            int x, int y, int z) {
        return null;
    }

    /** {@code LevelReader.getBiome(BlockPos)} (CraftEngine). */
    default net.minecraft.core.Holder<net.minecraft.world.level.biome.Biome> getBiome(
            net.minecraft.core.BlockPos pos) {
        return null;
    }

    default net.minecraft.world.level.biome.BiomeManager getBiomeManager() {
        return null;
    }

    default net.minecraft.core.Holder<net.minecraft.world.level.biome.Biome> getUncachedNoiseBiome(
            int x, int y, int z) {
        return null;
    }

    default boolean isClientSide() {
        return false;
    }

    default int getSeaLevel() {
        return 64;
    }

    default net.minecraft.world.level.dimension.DimensionType dimensionType() {
        return null;
    }

    default net.minecraft.core.RegistryAccess registryAccess() {
        return net.minecraft.core.RegistryAccess.EMPTY_REGISTRY_ACCESS();
    }

    default net.minecraft.world.flag.FeatureFlagSet enabledFeatures() {
        return net.minecraft.world.flag.FeatureFlagSet.of();
    }

    default net.minecraft.world.attribute.EnvironmentAttributeReader environmentAttributes() {
        return null;
    }
}
