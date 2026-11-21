package net.minecraft.world.item.component;

/** NMS {@code OminousBottleAmplifier} (EconomyShopGUI). */
public record OminousBottleAmplifier(int amplifier) {
    public static final int EFFECT_DURATION = 6000;
    public static final int MIN_AMPLIFIER = 0;
    public static final int MAX_AMPLIFIER = 4;
}
