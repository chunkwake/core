package net.minecraft.world.level.storage.loot.providers.number.ints;

/** NMS {@code ResolvableInt} (26.x). */
public interface ResolvableInt {
    int getInt(Object context);

    static ResolvableInt of(int value) {
        return new Constant(value);
    }

    record Constant(int value) implements ResolvableInt {
        @Override
        public int getInt(Object context) {
            return this.value;
        }
    }
}
