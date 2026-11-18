package com.mojang.serialization.codecs;

import com.mojang.serialization.DataResult;
import com.mojang.serialization.DynamicOps;

/** Рабочая реализация PrimitiveCodec (значение без преобразования). */
public final class SimplePrimitiveCodec<T> implements PrimitiveCodec<T> {
    @Override
    public DataResult<T> encodeStart(DynamicOps<Object> ops, T input) {
        return DataResult.success(input);
    }

    @Override
    public DataResult<T> parse(DynamicOps<Object> ops, Object input) {
        @SuppressWarnings("unchecked")
        T value = (T) input;
        return DataResult.success(value);
    }
}
