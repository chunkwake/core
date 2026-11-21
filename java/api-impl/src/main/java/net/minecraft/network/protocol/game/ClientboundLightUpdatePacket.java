package net.minecraft.network.protocol.game;

import java.util.BitSet;
import net.minecraft.network.protocol.Packet;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.lighting.LevelLightEngine;

/** NMS {@code ClientboundLightUpdatePacket} (CraftEngine). */
public class ClientboundLightUpdatePacket implements Packet {
    public ClientboundLightUpdatePacket() {
    }

    public ClientboundLightUpdatePacket(ChunkPos pos, LevelLightEngine lightEngine,
            BitSet skyLight, BitSet blockLight) {
    }
}
