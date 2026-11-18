package net.minecraft.network;

/**
 * Минимальный NMS-совместимый enum для ProtocolLib.
 *
 * <p>ProtocolLib ищет {@code network.EnumProtocol} → {@code network.ConnectionProtocol}
 * и строит соответствие по именам констант с {@code PacketType.Protocol}
 * (HANDSHAKING/PLAY/STATUS/LOGIN). Пока сервер не отдаёт настоящий NMS,
 * этого достаточно, чтобы плагин загрузился и продолжил инициализацию.
 */
public enum ConnectionProtocol {
    HANDSHAKING,
    PLAY,
    STATUS,
    LOGIN
}
