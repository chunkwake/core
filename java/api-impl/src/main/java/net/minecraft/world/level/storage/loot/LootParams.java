package net.minecraft.world.level.storage.loot;

/**
 * NMS {@code LootParams}: параметры вычисления дропа.
 *
 * <p>CraftEngine ищет {@code BlockStateBase.getDrops(LootParams.Builder)} по
 * точному типу аргумента, поэтому нужен вложенный {@code Builder}.
 */
public final class LootParams {
    public static final class Builder {
        public Builder withParameter(Object parameter, Object value) {
            return this;
        }

        public LootParams build() {
            return new LootParams();
        }
    }
}
