package net.minecraft.world.level.block;

import net.minecraft.sounds.SoundEvent;

/**
 * NMS {@code SoundType}: набор звуков блока.
 *
 * <p>CraftEngine создаёт объект через {@code SoundTypeProxy#newInstance};
 * proxy ищет конструктор с точными типами {@link SoundEvent}, поэтому
 * сигнатура должна совпадать с Paper, а не принимать {@code Object}.
 */
public class SoundType {
    /** Непустой sound type: CE читает звуки у каждого visual block state. */
    public static final SoundType DEFAULT = new SoundType(
            1.0F, 1.0F,
            new SoundEvent(net.minecraft.resources.Identifier.parse("minecraft:block.stone.break")),
            new SoundEvent(net.minecraft.resources.Identifier.parse("minecraft:block.stone.step")),
            new SoundEvent(net.minecraft.resources.Identifier.parse("minecraft:block.stone.place")),
            new SoundEvent(net.minecraft.resources.Identifier.parse("minecraft:block.stone.hit")),
            new SoundEvent(net.minecraft.resources.Identifier.parse("minecraft:block.stone.fall")));

    private final float volume;
    private final float pitch;
    private SoundEvent breakSound;
    private SoundEvent stepSound;
    private SoundEvent placeSound;
    private SoundEvent hitSound;
    private SoundEvent fallSound;

    public SoundType(float volume, float pitch, SoundEvent breakSound, SoundEvent stepSound,
            SoundEvent placeSound, SoundEvent hitSound, SoundEvent fallSound) {
        this.volume = volume;
        this.pitch = pitch;
        this.breakSound = breakSound;
        this.stepSound = stepSound;
        this.placeSound = placeSound;
        this.hitSound = hitSound;
        this.fallSound = fallSound;
    }

    public float getVolume() {
        return this.volume;
    }

    public float getPitch() {
        return this.pitch;
    }

    public SoundEvent getBreakSound() {
        return this.breakSound;
    }

    public void setBreakSound(SoundEvent sound) {
        this.breakSound = sound;
    }

    public SoundEvent getStepSound() {
        return this.stepSound;
    }

    public void setStepSound(SoundEvent sound) {
        this.stepSound = sound;
    }

    public SoundEvent getPlaceSound() {
        return this.placeSound;
    }

    public void setPlaceSound(SoundEvent sound) {
        this.placeSound = sound;
    }

    public SoundEvent getHitSound() {
        return this.hitSound;
    }

    public void setHitSound(SoundEvent sound) {
        this.hitSound = sound;
    }

    public SoundEvent getFallSound() {
        return this.fallSound;
    }

    public void setFallSound(SoundEvent sound) {
        this.fallSound = sound;
    }
}
