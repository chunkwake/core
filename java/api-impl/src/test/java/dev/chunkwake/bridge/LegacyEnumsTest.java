package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.bukkit.entity.EntityType;
import org.junit.jupiter.api.Test;

/** Алиасы старых имён enum'ов (Paper runtime patches). */
final class LegacyEnumsTest {
    @Test
    void entityTypeLegacyAliasResolves() {
        LegacyEnums.install();
        assertEquals(EntityType.END_CRYSTAL, EntityType.valueOf("ENDER_CRYSTAL"));
        assertEquals("END_CRYSTAL", EntityType.valueOf("ENDER_CRYSTAL").name());
    }
}
