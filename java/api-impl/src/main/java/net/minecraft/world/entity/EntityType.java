package net.minecraft.world.entity;

/** NMS {@code EntityType}: тип сущности (минимально). */
public class EntityType<T> {
    private final String id;

    public EntityType(String id) {
        this.id = id;
    }

    public String getId() {
        return this.id;
    }

    @Override
    public String toString() {
        return "EntityType[" + this.id + "]";
    }
}
