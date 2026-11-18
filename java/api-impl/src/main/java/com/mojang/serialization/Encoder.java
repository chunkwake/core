package com.mojang.serialization;

/** Минимальная заглушка DataFixerUpper. */
public interface Encoder<A> {
    DataResult<A> encodeStart(DynamicOps<Object> ops, A input);
}
