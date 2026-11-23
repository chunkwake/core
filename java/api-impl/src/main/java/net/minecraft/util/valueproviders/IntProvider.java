package net.minecraft.util.valueproviders;

import com.mojang.serialization.MapCodec;

/** NMS {@code IntProvider}: интерфейс int-провайдеров. */
public interface IntProvider {
    int sample(net.minecraft.util.RandomSource random);

    int minInclusive();

    int maxInclusive();

    MapCodec<? extends IntProvider> codec();
}
