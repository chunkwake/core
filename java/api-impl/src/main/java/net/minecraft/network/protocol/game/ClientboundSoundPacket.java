package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;

/** NMS {@code ClientboundSoundPacket} (поля для ProtocolLib). */
public class ClientboundSoundPacket implements Packet {
    private int soundId;
    private int source;
    private int x;
    private int y;
    private int z;
    private float volume;
    private float pitch;
    private long seed;

    public ClientboundSoundPacket() {
    }

    public ClientboundSoundPacket(int soundId, int source, int x, int y, int z, float volume,
            float pitch, long seed) {
        this.soundId = soundId;
        this.source = source;
        this.x = x;
        this.y = y;
        this.z = z;
        this.volume = volume;
        this.pitch = pitch;
        this.seed = seed;
    }

    public int soundId() {
        return this.soundId;
    }

    public int source() {
        return this.source;
    }

    public int x() {
        return this.x;
    }

    public int y() {
        return this.y;
    }

    public int z() {
        return this.z;
    }

    public float volume() {
        return this.volume;
    }

    public float pitch() {
        return this.pitch;
    }

    public long seed() {
        return this.seed;
    }
}
