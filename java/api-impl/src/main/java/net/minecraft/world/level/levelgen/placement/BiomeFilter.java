package net.minecraft.world.level.levelgen.placement;

import com.mojang.serialization.MapCodec;

/** NMS {@code BiomeFilter}: фильтр биома для placement. */
public class BiomeFilter implements PlacementFilter {
    public static final MapCodec<BiomeFilter> CODEC = MapCodec.unit(BiomeFilter::new);

    public BiomeFilter() {
    }
}
