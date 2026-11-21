package net.minecraft.network.protocol.game;

import java.util.BitSet;
import net.minecraft.network.protocol.Packet;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.lighting.LevelLightEngine;

/** NMS {@code ClientboundLevelChunkWithLightPacket} (CraftEngine). */
public class ClientboundLevelChunkWithLightPacket implements Packet {
    public ClientboundLevelChunkWithLightPacket() {
    }

    public ClientboundLevelChunkWithLightPacket(LevelChunk chunk, LevelLightEngine lightEngine,
            BitSet skyLight, BitSet blockLight) {
    }
}
