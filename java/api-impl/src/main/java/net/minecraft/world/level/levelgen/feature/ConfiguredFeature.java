package net.minecraft.world.level.levelgen.feature;

/** NMS {@code ConfiguredFeature}. */
public class ConfiguredFeature {
    public static final com.mojang.serialization.Codec<Object> CODEC =
            com.mojang.serialization.Codec.STRING.xmap(text -> (Object) new ConfiguredFeature(),
                    feature -> "configured_feature");

    public ConfiguredFeature() {
    }

    public com.mojang.serialization.Codec<Object> getCodec() {
        return CODEC;
    }

    public boolean place(net.minecraft.world.level.WorldGenLevel level,
            net.minecraft.world.level.chunk.ChunkGenerator chunkGenerator,
            net.minecraft.util.RandomSource random, net.minecraft.core.BlockPos pos) {
        return false;
    }
}
