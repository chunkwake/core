package com.mojang.serialization;

/** Простейший рабочий Codec: возвращает значение как есть. */
public final class SimpleCodec implements Codec<Object> {
    public static final SimpleCodec INSTANCE = new SimpleCodec();

    private SimpleCodec() {
    }

    @Override
    public DataResult<Object> encodeStart(DynamicOps<Object> ops, Object input) {
        return DataResult.success(input);
    }

    @Override
    public DataResult<Object> parse(DynamicOps<Object> ops, Object input) {
        return DataResult.success(input);
    }
}
