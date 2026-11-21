package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundRemoveEntitiesPacket}. */
public class ClientboundRemoveEntitiesPacket implements Packet {
    private final int[] ids;

    public ClientboundRemoveEntitiesPacket(it.unimi.dsi.fastutil.ints.IntList ids) {
        this(ids == null ? new int[0] : ids.toIntArray());
    }

    public ClientboundRemoveEntitiesPacket(int... ids) {
        this.ids = ids == null ? new int[0] : ids.clone();
    }

    public int[] ids() {
        return this.ids;
    }
}
