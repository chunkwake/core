package net.minecraft.network.protocol;

/**
 * Минимальный NMS-совместимый {@code network.protocol.PacketType}.
 *
 * <p>ProtocolLib 5.4.0 при инициализации ищет этот класс; конкретные
 * {@code *PacketTypes}/{@code *Protocols} он берёт через
 * {@code getNullableNMS} и пропускает отсутствующие. Пока сервер не отдаёт
 * реальные пакетные реестры, класс нужен только чтобы цепочка загрузки
 * ProtocolLib пошла дальше.
 */
public final class PacketType<T> {
    private PacketType() {
    }
}
