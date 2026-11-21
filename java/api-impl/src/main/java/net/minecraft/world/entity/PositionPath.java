package net.minecraft.world.entity;

import java.util.List;
import net.minecraft.world.phys.Vec3;

/** NMS {@code PositionPath} (CraftEngine). */
public interface PositionPath {
    /** NMS {@code PositionPath.Type}. */
    enum Type {
        DELTA,
        STEPPED
    }

    Vec3 endPosition();

    Type type();

    static PositionPath of(Vec3 endPosition) {
        return new PositionPath() {
            @Override
            public Vec3 endPosition() {
                return endPosition;
            }

            @Override
            public Type type() {
                return Type.DELTA;
            }
        };
    }

    static PositionPath stepped(List<Object> steps) {
        return of(Vec3.ZERO);
    }
}
