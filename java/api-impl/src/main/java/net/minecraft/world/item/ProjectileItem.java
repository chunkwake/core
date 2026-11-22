package net.minecraft.world.item;

/** NMS {@code ProjectileItem}. */
public interface ProjectileItem {
    default net.minecraft.world.entity.projectile.Projectile asProjectile(
            net.minecraft.world.level.Level level, net.minecraft.core.Position position,
            ItemStack stack, net.minecraft.core.Direction direction) {
        return null;
    }

    default DispenseConfig createDispenseConfig() {
        return DispenseConfig.DEFAULT;
    }

    default void shoot(net.minecraft.world.entity.projectile.Projectile projectile,
            double x, double y, double z, float power, float uncertainty) {
    }

    /** NMS {@code ProjectileItem.PositionFunction}. */
    interface PositionFunction {
        net.minecraft.core.Position getDispensePosition(Object source);
    }

    /** NMS {@code ProjectileItem.DispenseConfig}. */
    final class DispenseConfig {
        public static final DispenseConfig DEFAULT = new DispenseConfig();

        private final PositionFunction positionFunction;
        private final float uncertainty;
        private final float power;
        private final java.util.Optional<Integer> overrideDispenseEvent;

        public DispenseConfig() {
            this(null, 1.0F, 1.0F, java.util.Optional.empty());
        }

        public DispenseConfig(PositionFunction positionFunction, float uncertainty, float power,
                java.util.Optional<Integer> overrideDispenseEvent) {
            this.positionFunction = positionFunction;
            this.uncertainty = uncertainty;
            this.power = power;
            this.overrideDispenseEvent = overrideDispenseEvent;
        }

        public static Builder builder() {
            return new Builder();
        }

        public PositionFunction positionFunction() {
            return this.positionFunction;
        }

        public float uncertainty() {
            return this.uncertainty;
        }

        public float power() {
            return this.power;
        }

        public java.util.Optional<Integer> overrideDispenseEvent() {
            return this.overrideDispenseEvent;
        }

        public static final class Builder {
            public Builder positionFunction(PositionFunction function) {
                return this;
            }

            public Builder uncertainty(float uncertainty) {
                return this;
            }

            public Builder power(float power) {
                return this;
            }

            public Builder overrideDispenseEvent(int event) {
                return this;
            }

            public DispenseConfig build() {
                return DEFAULT;
            }
        }
    }
}
