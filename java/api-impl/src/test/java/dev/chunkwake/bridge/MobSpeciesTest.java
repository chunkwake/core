package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

import org.bukkit.entity.EntityType;
import org.bukkit.entity.Wolf;
import org.junit.jupiter.api.Test;

/** Виды, которые Rust умеет спавнить через {@code mob spawn}. */
final class MobSpeciesTest {
    @Test
    void mapsAllSupportedMobSpecies() {
        assertEquals("cow", MobImpl.species(EntityType.COW));
        assertEquals("wolf", MobImpl.species(EntityType.WOLF));
        assertEquals("piglin", MobImpl.species(EntityType.PIGLIN));
        assertEquals("iron_golem", MobImpl.species(EntityType.IRON_GOLEM));
        assertEquals("ender_dragon", MobImpl.species(EntityType.ENDER_DRAGON));
        assertEquals("cat", MobImpl.species(EntityType.CAT));
        assertEquals("bat", MobImpl.species(EntityType.BAT));
        assertEquals("wolf", MobImpl.species(Wolf.class));
    }

    @Test
    void rejectsEntitiesWithoutALivingRustSpecies() {
        assertNull(MobImpl.species(EntityType.ARMOR_STAND));
        assertNull(MobImpl.species(EntityType.END_CRYSTAL));
    }
}
