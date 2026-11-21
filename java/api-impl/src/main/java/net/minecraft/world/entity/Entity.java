package net.minecraft.world.entity;

import java.util.UUID;
import net.minecraft.core.RegistryAccess;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.storage.ValueInput;
import net.minecraft.world.level.storage.ValueOutput;
import net.minecraft.world.phys.Vec3;

/** NMS {@code Entity} (минимальная, но настоящая база). */
public class Entity implements net.minecraft.network.syncher.SyncedDataHolder {
    private static int nextId = 1;
    private final int id = nextId++;
    private final UUID uuid;
    private EntityType<?> type;
    protected Level level;
    private double x;
    private double y;
    private double z;
    private float yRot;
    private float xRot;
    private boolean removed;
    /** Поля, которые MythicMobs меняет рефлексией (setHitBox). */
    public EntityDimensions dimensions = EntityDimensions.scalable(0.6F, 1.8F);
    public float eyeHeight = 1.62F;
    public int tickCount;
    public net.minecraft.world.phys.AABB boundingBox =
            new net.minecraft.world.phys.AABB(0, 0, 0, 0, 0, 0);

    /** Метаданные Entity: id 0..7 как в реальном 26.3. */
    public static final net.minecraft.network.syncher.EntityDataAccessor<Byte>
            DATA_SHARED_FLAGS_ID = new net.minecraft.network.syncher.EntityDataAccessor<>(
                    0, net.minecraft.network.syncher.EntityDataSerializers.BYTE);
    public static final net.minecraft.network.syncher.EntityDataAccessor<Integer>
            DATA_AIR_SUPPLY_ID = new net.minecraft.network.syncher.EntityDataAccessor<>(
                    1, net.minecraft.network.syncher.EntityDataSerializers.INT);
    public static final net.minecraft.network.syncher.EntityDataAccessor<
            java.util.Optional<net.minecraft.network.chat.Component>> DATA_CUSTOM_NAME =
            new net.minecraft.network.syncher.EntityDataAccessor<>(2,
                    net.minecraft.network.syncher.EntityDataSerializers.OPTIONAL_COMPONENT);
    public static final net.minecraft.network.syncher.EntityDataAccessor<Boolean>
            DATA_CUSTOM_NAME_VISIBLE = new net.minecraft.network.syncher.EntityDataAccessor<>(
                    3, net.minecraft.network.syncher.EntityDataSerializers.BOOLEAN);
    public static final net.minecraft.network.syncher.EntityDataAccessor<Boolean>
            DATA_SILENT = new net.minecraft.network.syncher.EntityDataAccessor<>(
                    4, net.minecraft.network.syncher.EntityDataSerializers.BOOLEAN);
    public static final net.minecraft.network.syncher.EntityDataAccessor<Boolean>
            DATA_NO_GRAVITY = new net.minecraft.network.syncher.EntityDataAccessor<>(
                    5, net.minecraft.network.syncher.EntityDataSerializers.BOOLEAN);
    public static final net.minecraft.network.syncher.EntityDataAccessor<Object>
            DATA_POSE = new net.minecraft.network.syncher.EntityDataAccessor<>(
                    6, net.minecraft.network.syncher.EntityDataSerializers.POSE);
    public static final net.minecraft.network.syncher.EntityDataAccessor<Integer>
            DATA_TICKS_FROZEN = new net.minecraft.network.syncher.EntityDataAccessor<>(
                    7, net.minecraft.network.syncher.EntityDataSerializers.INT);

    public Entity() {
        this.uuid = UUID.randomUUID();
    }

    public Entity(EntityType<?> type, Level level) {
        this.uuid = UUID.randomUUID();
        this.type = type;
        this.level = level;
    }

    /** Конструктор с заданным UUID (CraftPlayer должен иметь UUID игрока). */
    protected Entity(UUID uuid) {
        this.uuid = uuid == null ? UUID.randomUUID() : uuid;
    }

    public int getId() {
        return this.id;
    }

    private final net.minecraft.network.syncher.SynchedEntityData entityData =
            new net.minecraft.network.syncher.SynchedEntityData();

    /** {@code Entity.getEntityData()} (LevelledMobs). */
    @Override
    public net.minecraft.network.syncher.SynchedEntityData getEntityData() {
        return this.entityData;
    }

    public UUID getUUID() {
        return this.uuid;
    }

    /** {@code Entity.getName()} (LevelledMobs). */
    public String getName() {
        return this.type == null ? "" : this.type.getId();
    }

    public String getTranslationKey() {
        return this.getName();
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

    public void setBoundingBox(net.minecraft.world.phys.AABB box) {
        this.boundingBox = box;
    }

    public net.minecraft.world.phys.AABB getBoundingBox() {
        return this.boundingBox;
    }

    /** Поле, которое ищет CraftEngine (EntityProxy#setZo). */
    public double zo;

    /** Поле, которое ищет CraftEngine (EntityProxy#getVehicle). */
    public Entity vehicle;

    /** Поле, которое ищет CraftEngine (EntityProxy#getPosition). */
    public Vec3 position = Vec3.ZERO;

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

    /** {@code Entity.isAlive()} (CraftEngine). */
    public boolean isAlive() {
        return !this.isRemoved();
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

    /** Paper: {@code Entity.getBukkitEntity()} возвращает CraftEntity. */
    public org.bukkit.craftbukkit.entity.CraftEntity getBukkitEntity() {
        return new org.bukkit.craftbukkit.entity.CraftEntity(this);
    }

    public void saveWithoutId(ValueOutput output) {
    }

    public void load(ValueInput input) {
    }

    public enum RemovalReason {
        KILLED, DISCARDED, UNLOADED_TO_CHUNK, UNLOADED_WITH_PLAYER, CHANGED_DIMENSION
    }
}
