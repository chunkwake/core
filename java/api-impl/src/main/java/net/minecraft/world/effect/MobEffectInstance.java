package net.minecraft.world.effect;

/** NMS {@code MobEffectInstance} (CraftEngine). */
public class MobEffectInstance {
    private int duration;
    private int amplifier;

    public MobEffectInstance() {
    }

    public MobEffectInstance(int duration, int amplifier) {
        this.duration = duration;
        this.amplifier = amplifier;
    }

    public MobEffectInstance(net.minecraft.core.Holder<MobEffect> effect) {
    }

    public MobEffectInstance(net.minecraft.core.Holder<MobEffect> effect, int duration) {
    }

    public MobEffectInstance(net.minecraft.core.Holder<MobEffect> effect, int duration,
            int amplifier) {
        this.duration = duration;
        this.amplifier = amplifier;
    }

    public int getDuration() {
        return this.duration;
    }

    public int getAmplifier() {
        return this.amplifier;
    }
}
