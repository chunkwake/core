package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

/** `Player.playSound(Entity, …)`: провод `sound_entity`. */
final class SoundWireTest {
    @Test
    void buildsEntitySoundRequest() {
        assertEquals(
                "soundent 42 minecraft:entity.cow.ambient MASTER 0.5 1.2 99",
                SoundWire.entity(42, "minecraft:entity.cow.ambient", "MASTER", 0.5F, 1.2F, 99L));
        assertEquals(
                "soundent 7 minecraft:block.note_block.harp BLOCKS 1.0 1.0 0",
                SoundWire.entity(7, "minecraft:block.note_block.harp", "BLOCKS", 1.0F, 1.0F, 0L));
    }
}
