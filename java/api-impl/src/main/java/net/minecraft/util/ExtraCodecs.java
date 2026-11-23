package net.minecraft.util;

import com.mojang.serialization.Codec;

/** NMS {@code ExtraCodecs}: дополнительные codec-константы. */
public final class ExtraCodecs {
    public static final Codec<String> NON_EMPTY_STRING = Codec.STRING;

    private ExtraCodecs() {
    }
}
