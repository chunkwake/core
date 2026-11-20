package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import net.minecraft.core.SimpleRegistry;
import net.minecraft.resources.Identifier;
import org.junit.jupiter.api.Test;

class VanillaRegistriesTest {
    @Test
    void biomeRegistryContainsVanillaKeys() {
        SimpleRegistry<Object> biome = VanillaRegistries.create("minecraft:biome");
        assertEquals(67, biome.size());
        assertTrue(biome.containsKey(Identifier.parse("minecraft:bamboo_jungle")));
        assertTrue(biome.containsKey(Identifier.parse("minecraft:plains")));
    }

    @Test
    void dimensionTypeRegistryContainsFourVanillaKeys() {
        SimpleRegistry<Object> dims = VanillaRegistries.create("minecraft:dimension_type");
        assertEquals(4, dims.size());
        assertTrue(dims.containsKey(Identifier.parse("minecraft:the_nether")));
    }
}
