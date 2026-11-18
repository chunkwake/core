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

    default <T> DataResult<T> encode(A input, DynamicOps<T> ops, T prefix) {
        @SuppressWarnings({"rawtypes", "unchecked"})
        DataResult<T> result = (DataResult) encodeStart((DynamicOps) ops, input);
        return result;
    }

    default <B> Codec<B> xmap(java.util.function.Function<? super A, ? extends B> to,
            java.util.function.Function<? super B, ? extends A> from) {
        Codec<A> self = this;
        return new Codec<B>() {
            @Override public DataResult<B> encodeStart(DynamicOps<Object> ops, B input) {
                return DataResult.success(input);
            }

            @Override public DataResult<B> parse(DynamicOps<Object> ops, Object input) {
                @SuppressWarnings("unchecked")
                B value = (B) input;
                return DataResult.success(value);
            }
        };
    }

    default MapCodec<A> fieldOf(String name) {
        return new MapCodec<A>() {
        };
    }

    default MapCodec<java.util.List<A>> listOf() {
        return new MapCodec<java.util.List<A>>() {
        };
    }
}
