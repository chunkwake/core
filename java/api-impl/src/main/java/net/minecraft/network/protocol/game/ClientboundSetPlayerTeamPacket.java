package net.minecraft.network.protocol.game;

/**
 * Заглушка NMS {@code ClientboundSetPlayerTeamPacket}: нужна для загрузки классов плагинов
 * (NpcPlugin и подобные ссылаются на пакеты в своих методах). Поведение
 * пакета в Chunkwake не используется — пакеты собираются в Rust.
 */
public class ClientboundSetPlayerTeamPacket {
    public ClientboundSetPlayerTeamPacket() {
    }
}
