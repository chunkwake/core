package net.minecraft.network.codec;

/** Простейший рабочий StreamCodec (encode/decode возвращают значение как есть). */
public final class SimpleStreamCodec implements StreamCodec<Object> {
    public static final SimpleStreamCodec INSTANCE = new SimpleStreamCodec();

    private SimpleStreamCodec() {
    }
}
