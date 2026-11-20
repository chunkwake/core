package net.minecraft.world.level.chunk;

import net.minecraft.core.IdMap;

/** NMS {@code Strategy}: стратегия палитры (упрощённо). */
public abstract class Strategy<T> {
    private final int entryCount;

    protected Strategy(int entryCount) {
        this.entryCount = entryCount;
    }

    public int entryCount() {
        return this.entryCount;
    }

    static <T> Strategy<T> createForBlockStates(IdMap<T> map) {
        return new Strategy<T>(4096) {

            @Override
            public String toString() {
                return "Strategy[blockStates]";
            }
        };
    }

    static <T> Strategy<T> createForBiomes() {
        return new Strategy<T>(64) {

            @Override
            public String toString() {
                return "Strategy[biomes]";
            }
        };
    }
}
