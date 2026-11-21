package net.minecraft.world.entity;

/** NMS {@code EntityType}: тип сущности (минимально). */
public class EntityType<T> {
    private final String id;
    private final int protocolId;

    public EntityType(String id) {
        this(id, -1);
    }

    public EntityType(String id, int protocolId) {
        this.id = id;
        this.protocolId = protocolId;
    }

    /** {@code EntityType.getDescriptionId()} (LevelledMobs). */
    public String getDescriptionId() {
        return "entity." + this.getId().replace(':', '.');
    }

    public String getTranslationKey() {
        return this.getDescriptionId();
    }

    /** Поле, которое ищет CraftEngine (EntityTypeProxy#getBuiltInRegistryHolder). */
    public final net.minecraft.core.Holder.Reference<EntityType<T>> builtInRegistryHolder =
            new net.minecraft.core.Holder.Reference<>(this);

    /** Поле, которое ищет CraftEngine (EntityTypeProxy#getDimensions). */
    public final net.minecraft.world.entity.EntityDimensions dimensions =
            net.minecraft.world.entity.EntityDimensions.scalable(0.6f, 1.8f);

    public String getId() {
        return this.id;
    }

    /** Id типа сущности в сети (реестр 26.3). */
    public int getProtocolId() {
        return this.protocolId;
    }

    /** Высота для spacing голограмм (DecentHolograms). */
    public double getHeight() {
        return this.id.endsWith("text_display") ? 0.5 : 1.975;
    }

    @Override
    public String toString() {
        return "EntityType[" + this.id + "]";
    }
}
