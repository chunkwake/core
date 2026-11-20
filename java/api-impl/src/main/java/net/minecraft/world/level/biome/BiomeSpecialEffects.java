package net.minecraft.world.level.biome;

import java.util.Optional;

/** NMS {@code BiomeSpecialEffects}: цвета биома. */
public class BiomeSpecialEffects {
    public static final BiomeSpecialEffects DEFAULT = new BiomeSpecialEffects();

    public BiomeSpecialEffects() {
    }

    public int waterColor() {
        return 0x3F76E4;
    }

    public Optional<Integer> grassColorOverride() {
        return Optional.empty();
    }

    public Optional<Integer> foliageColorOverride() {
        return Optional.empty();
    }

    public Optional<Integer> dryFoliageColorOverride() {
        return Optional.empty();
    }

    public GrassColorModifier grassColorModifier() {
        return GrassColorModifier.NONE;
    }

    /** NMS {@code BiomeSpecialEffects.GrassColorModifier}. */
    public enum GrassColorModifier {
        NONE {
            @Override
            public int modifyColor(double x, double z, int color) {
                return color;
            }
        },
        DARK_FOREST {
            @Override
            public int modifyColor(double x, double z, int color) {
                return color;
            }
        },
        SWAMP {
            @Override
            public int modifyColor(double x, double z, int color) {
                return color;
            }
        };

        public abstract int modifyColor(double x, double z, int color);
    }
}
