package net.minecraft.world.level.chunk;

public class PalettedContainerFactory {
    private final net.minecraft.world.level.chunk.Strategy<Object> blockStates =
            net.minecraft.world.level.chunk.Strategy.createForBlockStates(
                    new net.minecraft.core.IdMapper<>());
    private final net.minecraft.world.level.chunk.Strategy<Object> biomes =
            net.minecraft.world.level.chunk.Strategy.createForBiomes();

    public PalettedContainerFactory() {
    }

    public net.minecraft.world.level.block.state.BlockState defaultBlockState() {
        return net.minecraft.world.level.block.Blocks.AIR.defaultBlockState();
    }

    public net.minecraft.core.Holder defaultBiome() {
        return new net.minecraft.core.Holder.Reference<>(
                new net.minecraft.world.level.biome.Biome());
    }

    public net.minecraft.world.level.chunk.Strategy blockStatesStrategy() {
        return this.blockStates;
    }

    public net.minecraft.world.level.chunk.Strategy biomeStrategy() {
        return this.biomes;
    }
}