package net.minecraft.world.entity;

import java.util.UUID;
import net.minecraft.core.RegistryAccess;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.storage.ValueInput;
import net.minecraft.world.level.storage.ValueOutput;
import net.minecraft.world.phys.Vec3;

/** NMS {@code Entity} (минимальная, но настоящая база). */
public class Entity {
    private static int nextId = 1;
    private final int id = nextId++;
    private final UUID uuid = UUID.randomUUID();
    private EntityType<?> type;
    protected Level level;
    private double x;
    private double y;
    private double z;
    private float yRot;
    private float xRot;
    private boolean removed;

    /** ImageFrame: `Entity.DATA_SHARED_FLAGS_ID`. */
    public static final net.minecraft.network.syncher.EntityDataAccessor<Byte>
            DATA_SHARED_FLAGS_ID = new net.minecraft.network.syncher.EntityDataAccessor<>(
                    0, net.minecraft.network.syncher.EntityDataSerializers.BYTE);

    public Entity() {
    }

    public Entity(EntityType<?> type, Level level) {
        this.type = type;
        this.level = level;
    }

    public int getId() {
        return this.id;
    }

    public UUID getUUID() {
        return this.uuid;
    }

    public EntityType<?> getType() {
        return this.type;
    }

    public Level level() {
        return this.level;
    }

    public RegistryAccess registryAccess() {
        return RegistryAccess.EMPTY_REGISTRY_ACCESS();
    }

    public double getX() {
        return this.x;
    }

    public double getY() {
        return this.y;
    }

    public double getZ() {
        return this.z;
    }

    public void setPosRaw(double x, double y, double z) {
        this.x = x;
        this.y = y;
        this.z = z;
    }

    public void setPos(double x, double y, double z) {
        this.setPosRaw(x, y, z);
    }

    public Vec3 position() {
        return new Vec3(this.x, this.y, this.z);
    }

    public float getYRot() {
        return this.yRot;
    }

    public float getXRot() {
        return this.xRot;
    }

    public void setYRot(float yRot) {
        this.yRot = yRot;
    }

    public void setXRot(float xRot) {
        this.xRot = xRot;
    }

    public void setRot(float yRot, float xRot) {
        this.yRot = yRot;
        this.xRot = xRot;
    }

    public boolean isRemoved() {
        return this.removed;
    }

    public void setRemoved(RemovalReason reason) {
        this.removed = reason != null;
    }

    public void discard() {
        this.removed = true;
    }

    public org.bukkit.entity.Entity getBukkitEntity() {
        return null;
    }

    public void saveWithoutId(ValueOutput output) {
    }

    public void load(ValueInput input) {
    }

    public enum RemovalReason {
        KILLED, DISCARDED, UNLOADED_TO_CHUNK, UNLOADED_WITH_PLAYER, CHANGED_DIMENSION
    }
}
