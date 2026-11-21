package net.minecraft.network.protocol.game;

import net.minecraft.core.BlockPos;
import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundLevelEventPacket} (CraftEngine). */
public class ClientboundLevelEventPacket implements Packet {
    private int type;
    private BlockPos pos;
    private int data;
    private boolean globalEvent;

    public ClientboundLevelEventPacket() {
    }

    public ClientboundLevelEventPacket(int type, BlockPos pos, int data, boolean globalEvent) {
        this.type = type;
        this.pos = pos;
        this.data = data;
        this.globalEvent = globalEvent;
    }
}
