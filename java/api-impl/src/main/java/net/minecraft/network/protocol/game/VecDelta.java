package net.minecraft.network.protocol.game;

import net.minecraft.world.entity.PositionPath;

/** NMS {@code VecDelta} (26.x): дельта движения сущности. */
public interface VecDelta {
    VecDelta ZERO = new VecDelta() {
        @Override
        public int stepCount() {
            return 0;
        }

        @Override
        public boolean hasDeltaX() {
            return false;
        }

        @Override
        public boolean hasDeltaZ() {
            return false;
        }

        @Override
        public PositionPath decode(VecDeltaCodec codec) {
            return null;
        }
    };

    int stepCount();

    boolean hasDeltaX();

    boolean hasDeltaZ();

    PositionPath decode(VecDeltaCodec codec);

    /** Линейная дельта (пара шагов по осям). */
    record Linear(short xa, short ya, short za) implements VecDelta {
        @Override
        public int stepCount() {
            return 1;
        }

        @Override
        public boolean hasDeltaX() {
            return this.xa != 0;
        }

        @Override
        public boolean hasDeltaZ() {
            return this.za != 0;
        }

        @Override
        public PositionPath decode(VecDeltaCodec codec) {
            return null;
        }
    }
}
