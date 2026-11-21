package net.minecraft.sounds;

/** NMS-заглушка {@code SoundEvent} (CommandAPI проверяет наличие). */
public class SoundEvent {
    /** Поле, которое ищет CraftEngine (SoundEventProxy#getLocation). */
    private net.minecraft.resources.Identifier location;

    public SoundEvent() {
    }

    public SoundEvent(net.minecraft.resources.Identifier location) {
        this.location = location;
    }
}
