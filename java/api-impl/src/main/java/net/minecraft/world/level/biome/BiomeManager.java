package net.minecraft.world.level.biome;

/** Заглушка NMS BiomeManager (LevelReader.getBiomeManager). */
public class BiomeManager {
    private final BiomeResolver source;

    public BiomeManager() {
        this(null);
    }

    public BiomeManager(BiomeResolver source) {
        this.source = source;
    }

    public net.minecraft.core.Holder<Biome> getBiome(net.minecraft.core.BlockPos pos) {
        if (this.source == null) {
            return new net.minecraft.core.Holder.Reference<>(new Biome());
        }
        net.minecraft.core.Holder<Biome> biome =
                this.source.getNoiseBiome(pos.getX() >> 2, pos.getY() >> 2, pos.getZ() >> 2);
        return biome == null ? new net.minecraft.core.Holder.Reference<>(new Biome()) : biome;
    }

    public BiomeManager withDifferentSource(BiomeResolver source) {
        return new BiomeManager(source);
    }

    /** {@code BiomeManager.obfuscateSeed(long)} (Pl3xMap). */
    public static long obfuscateSeed(long seed) {
        return seed;
    }
}
