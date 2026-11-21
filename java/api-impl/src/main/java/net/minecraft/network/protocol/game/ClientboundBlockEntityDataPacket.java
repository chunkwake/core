package net.minecraft.network.protocol.game;

/**
 * Заглушка NMS {@code ClientboundBlockEntityDataPacket}: нужна для загрузки классов плагинов
 * (NpcPlugin и подобные ссылаются на пакеты в своих методах). Поведение
 * пакета в Chunkwake не используется — пакеты собираются в Rust.
 */
public class ClientboundBlockEntityDataPacket {
    public ClientboundBlockEntityDataPacket() {
    }

    public ClientboundBlockEntityDataPacket(net.minecraft.core.BlockPos pos,
            Object blockEntityType, net.minecraft.nbt.CompoundTag tag) {
    }
}
