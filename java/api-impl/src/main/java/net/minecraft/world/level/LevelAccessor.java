package net.minecraft.world.level;

import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.gameevent.GameEvent;

/** NMS {@code LevelAccessor}. */
public interface LevelAccessor extends LevelReader {
    default net.minecraft.util.RandomSource getRandom() {
        return null;
    }

    boolean setBlock(net.minecraft.core.BlockPos pos,
            net.minecraft.world.level.block.state.BlockState state, int flags);

    default void updateNeighborsAt(BlockPos pos, net.minecraft.world.level.block.Block block) {
    }

    default void levelEvent(Entity entity, int type, BlockPos pos, int data) {
    }

    default void levelEvent(int type, BlockPos pos, int data) {
    }

    default void playSound(Entity entity, BlockPos pos,
            net.minecraft.sounds.SoundEvent sound, net.minecraft.sounds.SoundSource source) {
    }

    default void playSound(Entity entity, BlockPos pos,
            net.minecraft.sounds.SoundEvent sound, net.minecraft.sounds.SoundSource source,
            float volume, float pitch) {
    }

    default void gameEvent(net.minecraft.core.Holder<GameEvent> event,
            net.minecraft.world.phys.Vec3 position, GameEvent.Context context) {
    }

    default void gameEvent(Entity entity, net.minecraft.core.Holder<GameEvent> event,
            net.minecraft.world.phys.Vec3 position) {
    }

    default void gameEvent(Entity entity, net.minecraft.core.Holder<GameEvent> event, BlockPos pos) {
    }

    default void scheduleTick(BlockPos pos, net.minecraft.world.level.block.Block block, int delay) {
    }

    default void scheduleTick(BlockPos pos, net.minecraft.world.level.block.Block block, int delay,
            net.minecraft.world.ticks.TickPriority priority) {
    }
}
