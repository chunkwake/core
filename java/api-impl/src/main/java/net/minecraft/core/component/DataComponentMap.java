package net.minecraft.core.component;

import java.util.Iterator;
import java.util.Set;

/** NMS {@code DataComponentMap}. */
public interface DataComponentMap extends Iterable<Object> {
    /** {@code DataComponentMap.builder()} (CraftEngine). */
    static Builder builder() {
        return new Builder() {
            @Override
            public DataComponentMap build() {
                return new PatchedDataComponentMap(null);
            }
        };
    }

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

    /** {@code DataComponentMap.Builder} (CraftEngine). */
    interface Builder {
        DataComponentMap build();
    }
}
