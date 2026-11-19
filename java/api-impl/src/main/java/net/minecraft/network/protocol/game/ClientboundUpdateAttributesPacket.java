package net.minecraft.network.protocol.game;

/**
 * Заглушка NMS {@code ClientboundUpdateAttributesPacket}: нужна для загрузки классов плагинов
 * (NpcPlugin и подобные ссылаются на пакеты в своих методах). Поведение
 * пакета в Chunkwake не используется — пакеты собираются в Rust.
 */
public class ClientboundUpdateAttributesPacket {
    public ClientboundUpdateAttributesPacket() {
    }
}
