package net.minecraft.world.entity;

/** NMS {@code EntitySpawnRequest}. */
public class EntitySpawnRequest {
    private final EntityType<?> type;

    public EntitySpawnRequest(EntityType<?> type) {
        this.type = type;
    }

    public EntityType<?> getType() {
        return this.type;
    }
}
