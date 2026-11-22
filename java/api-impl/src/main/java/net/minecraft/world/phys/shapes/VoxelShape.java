package net.minecraft.world.phys.shapes;

import net.minecraft.core.Direction;
import net.minecraft.world.phys.AABB;

/**
 * NMS {@code VoxelShape}: геометрия столкновений блока.
 *
 * <p>Chunkwake не считает формы блоков; для плагинов (CraftEngine) достаточно
 * существования типа и пустой формы.
 */
public class VoxelShape {
    public static final VoxelShape EMPTY = new VoxelShape();

    public VoxelShape() {
    }

    public boolean isEmpty() {
        return true;
    }

    public AABB bounds() {
        return new AABB(0, 0, 0, 0, 0, 0);
    }

    public double max(Direction.Axis axis) {
        return 0.0;
    }

    public VoxelShape move(double x, double y, double z) {
        return this;
    }

    @Override
    public String toString() {
        return "VoxelShape[empty]";
    }
}
