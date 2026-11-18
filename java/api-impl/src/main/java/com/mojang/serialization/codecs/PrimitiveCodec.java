package com.mojang.serialization.codecs;

import com.mojang.serialization.Codec;

/** Минимальный DFU {@code PrimitiveCodec}. */
public interface PrimitiveCodec<T> extends Codec<T> {
    default Codec<T> comapFlatMap(java.util.function.Function<?, ?> from,
            java.util.function.Function<?, ?> to) {
        return this;
    }
}
