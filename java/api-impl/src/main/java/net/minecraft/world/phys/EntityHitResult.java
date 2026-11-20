package net.minecraft.world.phys;

import net.minecraft.world.entity.Entity;

/** NMS {@code EntityHitResult}. */
public class EntityHitResult extends HitResult {
    private final Entity entity;

    public EntityHitResult(Entity entity) {
        this(entity, entity == null ? Vec3.ZERO : entity.position());
    }

    public EntityHitResult(Entity entity, Vec3 location) {
        super(location);
        this.entity = entity;
    }

    @Override
    public Type getType() {
        return Type.ENTITY;
    }

    public Entity getEntity() {
        return this.entity;
    }
}
