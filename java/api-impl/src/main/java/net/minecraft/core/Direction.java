package net.minecraft.core;

/** NMS {@code Direction} и {@code Direction.Axis}. */
public enum Direction {
    DOWN(0, -1, 0, Axis.Y),
    UP(1, 1, 0, Axis.Y),
    NORTH(2, 0, 0, Axis.Z),
    SOUTH(3, 0, 0, Axis.Z),
    WEST(4, 0, 0, Axis.X),
    EAST(5, 0, 0, Axis.X);

    private final int id;
    private final int stepX;
    private final int stepY;
    private final int stepZ;
    private final Axis axis;

    Direction(int id, int stepY, int stepZ, Axis axis) {
        this.id = id;
        this.stepX = axis == Axis.X ? (id == 5 ? 1 : -1) : 0;
        this.stepY = stepY;
        this.stepZ = axis == Axis.Z ? (id == 3 ? 1 : -1) : 0;
        this.axis = axis;
    }

    public int get3DDataValue() {
        return this.id;
    }

    public int getStepX() {
        return this.stepX;
    }

    public int getStepY() {
        return this.stepY;
    }

    public int getStepZ() {
        return this.stepZ;
    }

    public Axis getAxis() {
        return this.axis;
    }

    public Direction getOpposite() {
        return switch (this) {
            case DOWN -> UP;
            case UP -> DOWN;
            case NORTH -> SOUTH;
            case SOUTH -> NORTH;
            case WEST -> EAST;
            case EAST -> WEST;
        };
    }

    public static Direction from3DDataValue(int id) {
        return values()[Math.floorMod(id, values().length)];
    }

    public enum Axis {
        X, Y, Z
    }
}
