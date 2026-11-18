package net.minecraft.world.level.chunk;

import net.minecraft.core.IdMap;

/** NMS {@code Strategy}: стратегия палитры (упрощённо). */
public interface Strategy<T> {
    static <T> Strategy<T> createForBlockStates(IdMap<T> map) {
        return new Strategy<T>() {
            @Override
            public String toString() {
                return "Strategy[blockStates]";
            }
        };
    }
}
