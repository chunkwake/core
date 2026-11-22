package net.minecraft.world.level.storage.loot;

/**
 * NMS {@code LootParams}: параметры вычисления дропа.
 */
public final class LootParams {
    public static final class Builder {
        private final net.minecraft.server.level.ServerLevel level;

        public Builder(net.minecraft.server.level.ServerLevel level) {
            this.level = level;
        }

        public Builder() {
            this(null);
        }

        public net.minecraft.server.level.ServerLevel getLevel() {
            return this.level;
        }

        public <T> Builder withParameter(net.minecraft.util.context.ContextKey<T> key, T value) {
            return this;
        }

        public <T> Builder withOptionalParameter(net.minecraft.util.context.ContextKey<T> key, T value) {
            return this;
        }

        public <T> T getParameter(net.minecraft.util.context.ContextKey<T> key) {
            return null;
        }

        public <T> T getOptionalParameter(net.minecraft.util.context.ContextKey<T> key) {
            return null;
        }

        public Builder withLuck(float luck) {
            return this;
        }

        public Builder withParameter(Object parameter, Object value) {
            return this;
        }

        public LootParams create(net.minecraft.util.context.ContextKeySet params) {
            return new LootParams();
        }

        public LootParams create() {
            return new LootParams();
        }

        public LootParams build() {
            return create();
        }
    }
}
