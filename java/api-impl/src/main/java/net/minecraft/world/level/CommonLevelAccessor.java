package net.minecraft.world.level;

/**
 * Заглушка NMS CommonLevelAccessor: нужна squaremap (каст/instanceof и
 * чтение уровня через ServerLevel).
 */
public interface CommonLevelAccessor extends LevelReader, LevelSimulatedRW, EntityGetter {
}
