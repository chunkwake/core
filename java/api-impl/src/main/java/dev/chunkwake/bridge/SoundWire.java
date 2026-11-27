package dev.chunkwake.bridge;

/**
 * Провод звука, привязанного к сущности:
 * {@code player <uuid> soundent <entity> <sound> <source> <volume> <pitch> <seed>}.
 * Rust шлёт {@code sound_entity}: клиент ведёт звук за сущностью.
 */
final class SoundWire {
    private SoundWire() {
    }

    static String entity(int entity, String sound, String source,
            float volume, float pitch, long seed) {
        return "soundent " + entity + " " + sound + " " + source
                + " " + volume + " " + pitch + " " + seed;
    }
}
