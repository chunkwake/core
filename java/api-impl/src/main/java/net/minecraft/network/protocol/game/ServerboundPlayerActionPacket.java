package net.minecraft.network.protocol.game;

/**
 * Заглушка NMS {@code ServerboundPlayerActionPacket}: нужна для загрузки классов плагинов
 * (NpcPlugin и подобные ссылаются на пакеты в своих методах). Поведение
 * пакета в Chunkwake не используется — пакеты собираются в Rust.
 */
public class ServerboundPlayerActionPacket {
    /** Поле, которое ищет CraftEngine (getAction). */
    private Object action;
    /** Поле, которое ищет CraftEngine (getPos). */
    private net.minecraft.core.BlockPos pos = net.minecraft.core.BlockPos.ZERO;

    public ServerboundPlayerActionPacket() {
    }
}
