package net.minecraft.world.level.biome;

/** Заглушка NMS Biome. */
public class Biome {
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
}
