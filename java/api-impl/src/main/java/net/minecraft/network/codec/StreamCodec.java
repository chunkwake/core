package net.minecraft.network.codec;

/** Минимальный NMS {@code StreamCodec} (PacketEvents/ProtocolLib/DH). */
public interface StreamCodec<T> {
    /**
     * Декодирование из буфера (DecentHolograms: passengers).
     * По умолчанию — {@code null}; конкретные пакеты переопределяют.
     */
    default T decode(Object buffer) {
        return null;
    }
}
