package net.minecraft.util.valueproviders;

import com.mojang.serialization.Codec;

/** NMS {@code IntProviders}: codec-константы int-провайдеров. */
public final class IntProviders {
    public static final Codec<Integer> CODEC = Codec.INT;

    private IntProviders() {
    }
}
