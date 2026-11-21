package net.minecraft.network.protocol.game;

/**
 * Заглушка NMS {@code ServerboundPlayerActionPacket}: нужна для загрузки классов плагинов
 * (NpcPlugin и подобные ссылаются на пакеты в своих методах). Поведение
 * пакета в Chunkwake не используется — пакеты собираются в Rust.
 */
public class ServerboundPlayerActionPacket {
    /** Поле, которое ищет CraftEngine (getAction). */
    private Object action;

    public ServerboundPlayerActionPacket() {
    }
}
