package net.minecraft.world.level.biome;

/** Заглушка NMS Biome. */
public class Biome {
    public final ClimateSettings climateSettings =
            new ClimateSettings(true, 0.8F, TemperatureModifier.NONE, 0.4F);
    private final BiomeSpecialEffects specialEffects = new BiomeSpecialEffects();

    public Biome() {
    }

    /** {@code Biome.getSpecialEffects()} (squaremap читает цвета). */
    public BiomeSpecialEffects getSpecialEffects() {
        return this.specialEffects;
    }

    public float getBaseTemperature() {
        return 0.5F;
    }

    /** NMS {@code Biome.Precipitation}. */
    public enum Precipitation {
        NONE,
        RAIN,
        SNOW
    }

    /** NMS {@code Biome.TemperatureModifier}. */
    public enum TemperatureModifier {
        NONE,
        FROZEN
    }

    /** NMS {@code Biome.ClimateSettings}. */
    public static final class ClimateSettings {
        private final boolean hasPrecipitation;
        private final float temperature;
        private final TemperatureModifier temperatureModifier;
        private final float downfall;

        public ClimateSettings(boolean hasPrecipitation, float temperature,
                TemperatureModifier temperatureModifier, float downfall) {
            this.hasPrecipitation = hasPrecipitation;
            this.temperature = temperature;
            this.temperatureModifier = temperatureModifier;
            this.downfall = downfall;
        }

        public boolean hasPrecipitation() {
            return this.hasPrecipitation;
        }

        public float temperature() {
            return this.temperature;
        }

        public TemperatureModifier temperatureModifier() {
            return this.temperatureModifier;
        }

        public float downfall() {
            return this.downfall;
        }
    }
}
