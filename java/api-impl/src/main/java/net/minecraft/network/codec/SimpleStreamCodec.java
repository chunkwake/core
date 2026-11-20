package net.minecraft.network.codec;

/** Простейший рабочий StreamCodec (encode/decode возвращают значение как есть). */
public final class SimpleStreamCodec implements StreamCodec<Object> {
    public static final SimpleStreamCodec INSTANCE = new SimpleStreamCodec();

    /**
     * Поля, которые Emotecraft ищет через
     * {@code MethodHandles.Lookup.findVarHandle}: он «разворачивает»
     * анонимный StreamCodec, чтобы подменить fallback-провайдер для
     * неизвестных payload'ов. В настоящем Paper это синтетические поля
     * анонимного класса; у нас — обычные поля.
     */
    public StreamCodec<Object> this$0 = this;
    public net.minecraft.network.protocol.common.custom.CustomPacketPayload.FallbackProvider<?> val$fallback;

    private SimpleStreamCodec() {
    }
}
