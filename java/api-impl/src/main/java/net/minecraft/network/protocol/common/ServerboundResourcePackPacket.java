package net.minecraft.network.protocol.common;

import java.util.UUID;
import net.minecraft.network.protocol.Packet;

/** NMS {@code ServerboundResourcePackPacket} (CraftEngine). */
public class ServerboundResourcePackPacket implements Packet {
    /** NMS {@code ServerboundResourcePackPacket.Action}. */
    public enum Action {
        SUCCESSFULLY_LOADED,
        DECLINED,
        FAILED_DOWNLOAD,
        ACCEPTED,
        DOWNLOADED,
        INVALID_URL,
        DISCARDED
    }

    private UUID id;
    private Action action;

    public ServerboundResourcePackPacket() {
    }

    public ServerboundResourcePackPacket(Action action) {
        this.action = action;
    }

    public ServerboundResourcePackPacket(UUID id, Action action) {
        this.id = id;
        this.action = action;
    }

    public UUID getId() { return this.id; }
    public Action getAction() { return this.action; }
}
