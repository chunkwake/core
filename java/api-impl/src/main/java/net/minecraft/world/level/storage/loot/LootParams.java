package net.minecraft.world.level.storage.loot;

import java.util.LinkedHashMap;
import java.util.Map;
import net.minecraft.util.context.ContextKey;

/**
 * NMS {@code LootParams}: параметры вычисления дропа.
 *
 * <p>CraftEngine-интерсептор {@code getDrops} читает ORIGIN/TOOL/THIS_ENTITY
 * через {@code LootParams.Builder}; поэтому билдер должен реально хранить
 * параметры, а не возвращать null.
 */
public final class LootParams {
    private final Map<Object, Object> parameters;

    private LootParams(Map<Object, Object> parameters) {
        this.parameters = parameters;
    }

    public <T> T getParameter(ContextKey<T> key) {
        return value(key);
    }

    @SuppressWarnings("unchecked")
    private <T> T value(Object key) {
        return (T) this.parameters.get(key);
    }

    public static final class Builder {
        private final net.minecraft.server.level.ServerLevel level;
        private final Map<Object, Object> parameters = new LinkedHashMap<>();

        public Builder(net.minecraft.server.level.ServerLevel level) {
            this.level = level;
        }

        public Builder() {
            this(null);
        }

        public net.minecraft.server.level.ServerLevel getLevel() {
            return this.level;
        }

        public <T> Builder withParameter(ContextKey<T> key, T value) {
            this.parameters.put(key, value);
            return this;
        }

        public <T> Builder withOptionalParameter(ContextKey<T> key, T value) {
            if (value != null) {
                this.parameters.put(key, value);
            }
            return this;
        }

        public <T> T getParameter(ContextKey<T> key) {
            return this.value(key);
        }

        public <T> T getOptionalParameter(ContextKey<T> key) {
            return this.value(key);
        }

        @SuppressWarnings("unchecked")
        private <T> T value(Object key) {
            return key == null ? null : (T) this.parameters.get(key);
        }

        /** Вариант без generic-типа: CE-прокси передаёт ключи как Object. */
        public Builder withParameter(Object key, Object value) {
            if (key != null && value != null) {
                this.parameters.put(key, value);
            }
            return this;
        }

        public Builder withOptionalParameter(Object key, Object value) {
            if (key != null && value != null) {
                this.parameters.put(key, value);
            }
            return this;
        }

        @SuppressWarnings("unchecked")
        public <T> T getOptionalParameter(Object key) {
            return key == null ? null : (T) this.parameters.get(key);
        }

        public Builder withLuck(float luck) {
            return this;
        }

        public LootParams create(net.minecraft.util.context.ContextKeySet params) {
            return new LootParams(this.parameters);
        }

        public LootParams create() {
            return new LootParams(this.parameters);
        }

        public LootParams build() {
            return create();
        }
    }
}
