package net.minecraft.world.phys;

/** NMS {@code Vec2}: поворот (yaw/pitch) — CommandAPI читает поля. */
public class Vec2 {
    public final float x;
    public final float y;

    public Vec2() {
        this(0.0F, 0.0F);
    }

    public Vec2(float x, float y) {
        this.x = x;
        this.y = y;
    }

    public float x() {
        return this.x;
    }

    public float y() {
        return this.y;
    }
}
