package net.minecraft.network.codec;

/** Минимальный NMS {@code StreamCodec} (PacketEvents/ProtocolLib/DH). */
public interface StreamCodec<T> {
    /** {@code StreamCodec.unit(V)} (CraftEngine). */
    static <T> StreamCodec<T> unit(T value) {
        return new StreamCodec<T>() {
        };
    }

    /**
     * Декодирование из буфера (DecentHolograms: passengers).
     * По умолчанию — {@code null}; конкретные пакеты переопределяют.
     */
    default T decode(Object buffer) {
        return null;
    }
}
