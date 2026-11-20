package net.minecraft.world.level.biome;

import java.util.Set;
import net.minecraft.core.Holder;

/** NMS {@code BiomeResolver}: поставщик биомов (squaremap ChunkSnapshot). */
public interface BiomeResolver {
    Holder<Biome> getNoiseBiome(int x, int y, int z);

    default Set<Holder<Biome>> getBiomesWithin(int x, int y, int z, int radius) {
        return Set.of();
    }
}
