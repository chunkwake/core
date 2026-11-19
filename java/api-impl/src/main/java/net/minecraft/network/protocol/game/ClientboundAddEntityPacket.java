package net.minecraft.network.protocol.game;

import java.util.UUID;
import net.minecraft.network.protocol.Packet;

/**
 * NMS {@code ClientboundAddEntityPacket}.
 *
 * <p>Скорость (`LpVec3`) хранится сырыми байтами: для наших спавнов она
 * нулевая (один байт 0), а нестандартные значения мост пропускает без полей.
 */
public class ClientboundAddEntityPacket implements Packet {
    private int id;
    private UUID uuid;
    private int type;
    private double x;
    private double y;
    private double z;
    private byte[] velocity = {0};
    private int pitch;
    private int yaw;
    private int headYaw;
    private int data;

    public ClientboundAddEntityPacket() {
    }

    /**
     * Сигнатура, которую использует DecentHolograms:
     * (id, uuid, x, y, z, pitch, yaw, type, data, velocity, headYaw).
     */
    public ClientboundAddEntityPacket(int id, UUID uuid, double x, double y, double z,
            float pitch, float yaw, int type, int data,
            net.minecraft.world.phys.Vec3 velocity, double headYaw) {
        this.id = id;
        this.uuid = uuid;
        this.type = type;
        this.x = x;
        this.y = y;
        this.z = z;
        this.pitch = angle(pitch);
        this.yaw = angle(yaw);
        this.headYaw = angle((float) headYaw);
        this.data = data;
    }

    private static int angle(float degrees) {
        return ((int) Math.floor(degrees * 256.0 / 360.0)) & 0xFF;
    }

    public ClientboundAddEntityPacket(int id, UUID uuid, int type, double x, double y, double z,
            byte[] velocity, int pitch, int yaw, int headYaw, int data) {
        this.id = id;
        this.uuid = uuid;
        this.type = type;
        this.x = x;
        this.y = y;
        this.z = z;
        this.velocity = velocity == null ? new byte[] {0} : velocity;
        this.pitch = pitch;
        this.yaw = yaw;
        this.headYaw = headYaw;
        this.data = data;
    }

    public int id() {
        return this.id;
    }

    public UUID uuid() {
        return this.uuid;
    }

    public int type() {
        return this.type;
    }

    public double x() {
        return this.x;
    }

    public double y() {
        return this.y;
    }

    public double z() {
        return this.z;
    }

    public byte[] velocity() {
        return this.velocity;
    }

    public int pitch() {
        return this.pitch;
    }

    public int yaw() {
        return this.yaw;
    }

    public int headYaw() {
        return this.headYaw;
    }

    public int data() {
        return this.data;
    }
}
