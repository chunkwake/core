package net.minecraft.world.entity;

import net.minecraft.world.phys.Vec3;

/** NMS {@code PositionMoveRotation}: позиция + скорость + повороты. */
public final class PositionMoveRotation {
    private final Vec3 position;
    private final Vec3 deltaMovement;
    private final float yRot;
    private final float xRot;

    public PositionMoveRotation(Vec3 position, Vec3 deltaMovement, float yRot, float xRot) {
        this.position = position;
        this.deltaMovement = deltaMovement;
        this.yRot = yRot;
        this.xRot = xRot;
    }

    public Vec3 position() {
        return this.position;
    }

    public Vec3 deltaMovement() {
        return this.deltaMovement;
    }

    public float yRot() {
        return this.yRot;
    }

    public float xRot() {
        return this.xRot;
    }
}
