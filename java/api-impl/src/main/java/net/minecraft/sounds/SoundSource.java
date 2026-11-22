package net.minecraft.sounds;

/** NMS {@code SoundSource} (CraftEngine). */
public enum SoundSource {
    MASTER,
    MUSIC,
    RECORD,
    WEATHER,
    BLOCK,
    HOSTILE,
    NEUTRAL,
    PLAYER,
    AMBIENT,
    VOICE;

    public final String name;

    SoundSource() {
        this.name = name().toLowerCase(java.util.Locale.ROOT);
    }

    public String getName() {
        return this.name().toLowerCase(java.util.Locale.ROOT);
    }
}
