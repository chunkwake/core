package com.mojang.serialization;

/**
 * Минимальная заглушка DataFixerUpper: ProtocolLib ищет этот класс при
 * инициализации обёрток (Paper несёт его в libraries, у нас — только api).
 */
public interface Codec<A> extends Encoder<A>, Decoder<A> {
    com.mojang.serialization.codecs.PrimitiveCodec<String> STRING =
            new com.mojang.serialization.codecs.SimplePrimitiveCodec<>();
    com.mojang.serialization.codecs.PrimitiveCodec<Boolean> BOOL =
            new com.mojang.serialization.codecs.SimplePrimitiveCodec<>();
    com.mojang.serialization.codecs.PrimitiveCodec<Integer> INT =
            new com.mojang.serialization.codecs.SimplePrimitiveCodec<>();
    com.mojang.serialization.codecs.PrimitiveCodec<Long> LONG =
            new com.mojang.serialization.codecs.SimplePrimitiveCodec<>();
    com.mojang.serialization.codecs.PrimitiveCodec<Float> FLOAT =
            new com.mojang.serialization.codecs.SimplePrimitiveCodec<>();
    com.mojang.serialization.codecs.PrimitiveCodec<Double> DOUBLE =
            new com.mojang.serialization.codecs.SimplePrimitiveCodec<>();
}
