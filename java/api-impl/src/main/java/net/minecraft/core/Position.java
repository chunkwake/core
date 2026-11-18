package net.minecraft.core;

/** NMS {@code Position}. */
public interface Position {
    double x();

    double y();

    double z();

    default BlockPos blockPosition() {
        return BlockPos.containing(this.x(), this.y(), this.z());
    }
}
