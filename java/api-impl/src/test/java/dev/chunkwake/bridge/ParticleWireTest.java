package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

import org.bukkit.Color;
import org.bukkit.Particle;
import org.junit.jupiter.api.Test;

/** Тело сетевых данных частиц (dust/block/item). */
final class ParticleWireTest {
    @Test
    void dustBodyIsArgbAndScale() {
        String body = ParticleWire.body(Particle.DUST,
                new Particle.DustOptions(Color.RED, 1.5F));
        assertEquals("ffff00003fc00000", body);
    }

    @Test
    void simpleParticleHasNoBody() {
        assertEquals("", ParticleWire.body(Particle.FLAME, null));
    }

    @Test
    void complexParticleWithoutDataIsSkipped() {
        assertNull(ParticleWire.body(Particle.DUST, null));
    }
}
