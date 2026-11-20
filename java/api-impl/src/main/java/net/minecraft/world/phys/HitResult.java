package net.minecraft.world.phys;

import net.minecraft.world.entity.Entity;

/** NMS {@code HitResult}: результат рейкаста. */
public abstract class HitResult {
    public enum Type {
        MISS,
        BLOCK,
        ENTITY
    }

    protected final Vec3 location;

    protected HitResult(Vec3 location) {
        this.location = location == null ? Vec3.ZERO : location;
    }

    public double distanceTo(Entity entity) {
        return this.location.distanceTo(entity == null ? Vec3.ZERO : entity.position());
    }

    public abstract Type getType();

    public Vec3 getLocation() {
        return this.location;
    }
}
