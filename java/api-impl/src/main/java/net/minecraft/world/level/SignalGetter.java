package net.minecraft.world.level;

import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;

/** NMS {@code SignalGetter}: чтение редстоун-сигналов. */
public interface SignalGetter extends BlockGetter {
    Direction[] DIRECTIONS = Direction.values();

    default int getDirectSignal(BlockPos pos, Direction direction) {
        return 0;
    }

    default int getDirectSignalTo(BlockPos pos) {
        return 0;
    }

    default int getControlInputSignal(BlockPos pos, Direction direction, boolean diodesOnly) {
        return 0;
    }

    default boolean hasSignal(BlockPos pos, Direction direction) {
        return false;
    }

    default int getSignal(BlockPos pos, Direction direction) {
        return 0;
    }

    default int getBestOwnOrNeighbourSignal(BlockPos pos) {
        return 0;
    }

    default boolean hasNeighborSignal(BlockPos pos) {
        return false;
    }

    default int getBestNeighborSignal(BlockPos pos) {
        return 0;
    }
}
