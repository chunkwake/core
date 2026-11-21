package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;
import net.minecraft.resources.Identifier;
import net.minecraft.sounds.SoundSource;

/** NMS {@code ClientboundStopSoundPacket} (CraftEngine). */
public class ClientboundStopSoundPacket implements Packet {
    private Identifier name;
    private SoundSource source;

    public ClientboundStopSoundPacket() {
    }

    public ClientboundStopSoundPacket(Identifier name, SoundSource source) {
        this.name = name;
        this.source = source;
    }

    public ClientboundStopSoundPacket(SoundSource source, Identifier name) {
        this.name = name;
        this.source = source;
    }
}
