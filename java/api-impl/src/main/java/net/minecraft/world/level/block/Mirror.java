package net.minecraft.world.level.block;

import net.minecraft.core.Direction;

/** NMS {@code Mirror}. */
public enum Mirror {
    NONE, LEFT_RIGHT, FRONT_BACK;

    public Direction mirror(Direction direction) {
        if (this == NONE) {
            return direction;
        }
        if (direction.getAxis() == Direction.Axis.Y) {
            return direction;
        }
        boolean eastWest = direction.getAxis() == Direction.Axis.X;
        if (this == FRONT_BACK && !eastWest) {
            return direction.getOpposite();
        }
        if (this == LEFT_RIGHT && eastWest) {
            return direction.getOpposite();
        }
        return direction;
    }
}
