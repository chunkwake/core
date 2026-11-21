package net.minecraft.network.protocol.game;

/**
 * Заглушка NMS {@code ClientboundUpdateAttributesPacket}: нужна для загрузки классов плагинов
 * (NpcPlugin и подобные ссылаются на пакеты в своих методах). Поведение
 * пакета в Chunkwake не используется — пакеты собираются в Rust.
 */
public class ClientboundUpdateAttributesPacket {
    public ClientboundUpdateAttributesPacket() {
    }

    public ClientboundUpdateAttributesPacket(int entityId,
            java.util.Collection<net.minecraft.world.entity.ai.attributes.AttributeInstance> values) {
    }

    public ClientboundUpdateAttributesPacket(int entityId,
            java.util.List<net.minecraft.world.entity.ai.attributes.AttributeInstance> values) {
    }
}
