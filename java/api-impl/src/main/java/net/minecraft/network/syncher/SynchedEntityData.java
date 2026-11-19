package net.minecraft.network.syncher;

import java.util.HashMap;
import java.util.Map;

/** NMS {@code SynchedEntityData} (минимально) + DataValue/DataItem. */
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

    /** Значение метаданных в сетевом виде (id + serializer + value). */
    public static final class DataValue<T> {
        private final int id;
        private final EntityDataSerializer<T> serializer;
        private final T value;

        public DataValue(int id, EntityDataSerializer<T> serializer, T value) {
            this.id = id;
            this.serializer = serializer;
            this.value = value;
        }

        public int id() {
            return this.id;
        }

        public EntityDataSerializer<T> serializer() {
            return this.serializer;
        }

        public T value() {
            return this.value;
        }
    }

    /** Пара accessor→value; {@link #value()} даёт {@link DataValue}. */
    public static final class DataItem<T> {
        private final EntityDataAccessor<T> accessor;
        private final T value;

        public DataItem(EntityDataAccessor<T> accessor, T value) {
            this.accessor = accessor;
            this.value = value;
        }

        public EntityDataAccessor<T> accessor() {
            return this.accessor;
        }

        public T rawValue() {
            return this.value;
        }

        public DataValue<T> value() {
            return new DataValue<>(this.accessor.getId(), this.accessor.getSerializer(),
                    this.value);
        }
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
