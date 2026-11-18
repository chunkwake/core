package net.minecraft.core;

import java.util.ArrayList;

/**
 * NMS {@code NonNullList}: ProtocolLib ищет статический {@code create()},
 * возвращающий этот же тип.
 */
public class NonNullList<E> extends ArrayList<E> {
    public static <E> NonNullList<E> create() {
        return new NonNullList<>();
    }
}
