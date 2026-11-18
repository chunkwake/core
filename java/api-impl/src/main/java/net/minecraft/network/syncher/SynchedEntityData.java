package net.minecraft.network.syncher;

import java.util.HashMap;
import java.util.Map;

/** NMS {@code SynchedEntityData} (минимально). */
public class SynchedEntityData {
    private final Map<EntityDataAccessor<?>, Object> values = new HashMap<>();

    public <T> T get(EntityDataAccessor<T> accessor) {
        @SuppressWarnings("unchecked")
        T value = (T) this.values.get(accessor);
        return value;
    }

    public <T> void set(EntityDataAccessor<T> accessor, T value) {
        this.values.put(accessor, value);
    }

    public static final class Builder {
        public <T> Builder define(EntityDataAccessor<T> accessor, T value) {
            return this;
        }

        public SynchedEntityData build() {
            return new SynchedEntityData();
        }
    }
}
