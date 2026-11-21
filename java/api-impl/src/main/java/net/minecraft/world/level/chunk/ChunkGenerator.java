package net.minecraft.world.level.chunk;

/** NMS {@code ChunkGenerator} (CraftEngine). */
public class ChunkGenerator {
    /** Поле, которое ищет CraftEngine (ChunkGeneratorProxy#getBiomeSource). */
    private final net.minecraft.world.level.biome.BiomeSource biomeSource =
            new net.minecraft.world.level.biome.BiomeSource();

    public ChunkGenerator() {
    }

    public net.minecraft.world.level.biome.BiomeSource getBiomeSource() {
        return this.biomeSource;
    }
}
