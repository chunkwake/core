package net.minecraft.world.level.storage.loot.providers.number.floats;

/** NMS {@code ResolvableFloat} (26.x): провайдер числа в loot-таблицах. */
public interface ResolvableFloat {
    float getFloat(Object context);

    static ResolvableFloat of(float value) {
        return new Constant(value);
    }

    /** Константный провайдер. */
    record Constant(float value) implements ResolvableFloat {
        @Override
        public float getFloat(Object context) {
            return this.value;
        }
    }
}
