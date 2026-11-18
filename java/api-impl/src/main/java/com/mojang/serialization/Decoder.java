package com.mojang.serialization;

/** Минимальная заглушка DataFixerUpper. */
public interface Decoder<A> {
    DataResult<A> parse(DynamicOps<Object> ops, Object input);
}
