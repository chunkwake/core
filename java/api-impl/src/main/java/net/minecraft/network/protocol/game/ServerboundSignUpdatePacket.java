package net.minecraft.network.protocol.game;

/**
 * Заглушка NMS {@code ServerboundSignUpdatePacket}: нужна для загрузки классов плагинов
 * (NpcPlugin и подобные ссылаются на пакеты в своих методах). Поведение
 * пакета в Chunkwake не используется — пакеты собираются в Rust.
 */
public class ServerboundSignUpdatePacket {
    /** Поле, которое ищет CraftEngine (getLines). */
    private String[] lines = new String[0];

    public ServerboundSignUpdatePacket() {
    }
}
