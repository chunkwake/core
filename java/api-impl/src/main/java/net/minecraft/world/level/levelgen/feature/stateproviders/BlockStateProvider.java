package net.minecraft.world.level.levelgen.feature.stateproviders;

import com.mojang.serialization.Codec;

/** NMS {@code BlockStateProvider}: интерфейс state-provider'ов. */
public interface BlockStateProvider {
    Codec<BlockStateProvider> CODEC = Codec.STRING.xmap(
            text -> (BlockStateProvider) null,
            provider -> "minecraft:stone");
}
