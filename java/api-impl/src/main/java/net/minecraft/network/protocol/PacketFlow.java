package net.minecraft.network.protocol;

/**
 * Минимальный NMS-совместимый enum для ProtocolLib
 * ({@code MinecraftReflection.getEnumProtocolDirectionClass()}).
 */
public enum PacketFlow {
    SERVERBOUND,
    CLIENTBOUND
}
