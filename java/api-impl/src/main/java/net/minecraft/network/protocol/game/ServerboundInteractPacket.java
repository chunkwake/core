package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;

/** NMS {@code ServerboundInteractPacket} (ProtocolLib EnumWrappers). */
public class ServerboundInteractPacket implements Packet {
    /** Первое ссылочное поле: ProtocolLib выводит из него {@code Action}. */
    private Action action;
    private int entityId;

    public ServerboundInteractPacket() {
    }

    public ServerboundInteractPacket(int entityId) {
        this.entityId = entityId;
    }

    public int entityId() {
        return this.entityId;
    }

    public Action action() {
        return this.action;
    }

    /** NMS {@code ServerboundInteractPacket.Action}. */
    public enum Action {
        INTERACT,
        ATTACK,
        INTERACT_AT
    }
}
