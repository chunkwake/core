package net.minecraft.core.component;

import java.util.Iterator;
import java.util.Set;

/** NMS {@code DataComponentMap}. */
public interface DataComponentMap extends Iterable<Object> {
    <T> T get(DataComponentType<? extends T> type);

    default boolean has(DataComponentType<?> type) {
        return this.get(type) != null;
    }

    Set<DataComponentType<?>> keySet();

    int size();

    DataComponentPatch asPatch();

    @Override
    default Iterator<Object> iterator() {
        return java.util.Collections.emptyIterator();
    }
}
