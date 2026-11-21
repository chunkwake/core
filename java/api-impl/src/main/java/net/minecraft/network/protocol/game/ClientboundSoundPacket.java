package net.minecraft.network.protocol.game;

import net.minecraft.core.Holder;
import net.minecraft.network.protocol.Packet;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundSource;

/** NMS {@code ClientboundSoundPacket} (CraftEngine). */
public class ClientboundSoundPacket implements Packet {
    private Holder<SoundEvent> sound;
    private SoundSource source;
    private double x;
    private double y;
    private double z;
    private float volume;
    private float pitch;
    private long seed;

    public ClientboundSoundPacket() {
    }

    public ClientboundSoundPacket(Holder<SoundEvent> sound, SoundSource source,
            double x, double y, double z, float volume, float pitch, long seed) {
        this.sound = sound;
        this.source = source;
        this.x = x;
        this.y = y;
        this.z = z;
        this.volume = volume;
        this.pitch = pitch;
        this.seed = seed;
    }
}
