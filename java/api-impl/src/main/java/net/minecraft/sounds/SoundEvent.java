package net.minecraft.sounds;

/** NMS-заглушка {@code SoundEvent} (CommandAPI проверяет наличие). */
public class SoundEvent {
    /** Поле, которое ищет CraftEngine (SoundEventProxy#getLocation). */
    private net.minecraft.resources.Identifier location;
    private java.util.Optional<Float> fixedRange = java.util.Optional.empty();

    public SoundEvent() {
    }

    public SoundEvent(net.minecraft.resources.Identifier location) {
        this.location = location;
    }

    public SoundEvent(net.minecraft.resources.Identifier location,
            java.util.Optional<Float> fixedRange) {
        this.location = location;
        this.fixedRange = fixedRange == null ? java.util.Optional.empty() : fixedRange;
    }

    public static SoundEvent create(net.minecraft.resources.Identifier location,
            java.util.Optional<Float> fixedRange) {
        return new SoundEvent(location, fixedRange);
    }

    public static SoundEvent createVariableRangeEvent(net.minecraft.resources.Identifier location) {
        return new SoundEvent(location);
    }

    public static SoundEvent createFixedRangeEvent(net.minecraft.resources.Identifier location,
            float range) {
        return new SoundEvent(location, java.util.Optional.of(range));
    }

    public java.util.Optional<Float> fixedRange() {
        return this.fixedRange;
    }

    public float getRange(float volume) {
        return volume;
    }
}
