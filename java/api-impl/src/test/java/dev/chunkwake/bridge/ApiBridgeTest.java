package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;

import io.papermc.paper.InternalAPIBridge;
import org.bukkit.entity.EntityType;
import org.bukkit.entity.SpawnCategory;
import org.junit.jupiter.api.Test;

/** {@code InternalAPIBridge} для {@code EntityType}: ключи перевода и категории спавна. */
class ApiBridgeTest {
    private static final InternalAPIBridge BRIDGE = InternalAPIBridge.get();

    @Test
    void translationKeysMatchVanilla() {
        assertEquals("entity.minecraft.zombie", BRIDGE.getTranslationKey(EntityType.ZOMBIE));
        assertEquals("entity.minecraft.cow", BRIDGE.getTranslationKey(EntityType.COW));
        assertEquals("entity.minecraft.iron_golem", BRIDGE.getTranslationKey(EntityType.IRON_GOLEM));
    }

    @Test
    void spawnCategoriesMatchVanillaMobCategory() {
        assertEquals(SpawnCategory.MONSTER, BRIDGE.getSpawnCategory(EntityType.ZOMBIE));
        assertEquals(SpawnCategory.ANIMAL, BRIDGE.getSpawnCategory(EntityType.COW));
        assertEquals(SpawnCategory.AMBIENT, BRIDGE.getSpawnCategory(EntityType.BAT));
        assertEquals(SpawnCategory.WATER_ANIMAL, BRIDGE.getSpawnCategory(EntityType.SQUID));
        assertEquals(SpawnCategory.WATER_AMBIENT, BRIDGE.getSpawnCategory(EntityType.COD));
        assertEquals(SpawnCategory.WATER_UNDERGROUND_CREATURE, BRIDGE.getSpawnCategory(EntityType.GLOW_SQUID));
        assertEquals(SpawnCategory.AXOLOTL, BRIDGE.getSpawnCategory(EntityType.AXOLOTL));
        assertEquals(SpawnCategory.MISC, BRIDGE.getSpawnCategory(EntityType.IRON_GOLEM));
        assertEquals(SpawnCategory.MISC, BRIDGE.getSpawnCategory(EntityType.VILLAGER));
        assertEquals(SpawnCategory.MISC, BRIDGE.getSpawnCategory(EntityType.ARMOR_STAND));
    }

    @Test
    void everyEntityTypeHasCategory() {
        for (EntityType type : EntityType.values()) {
            if (type == EntityType.UNKNOWN) {
                continue;
            }
            assertNotNull(BRIDGE.getSpawnCategory(type), type.name());
        }
    }

    @Test
    void damageEffectsAndOccupanciesFromVanillaRegistry() {
        assertEquals(org.bukkit.damage.DamageEffect.HURT, BRIDGE.getDamageEffect("hurt"));
        assertEquals(org.bukkit.damage.DamageEffect.THORNS, BRIDGE.getDamageEffect("thorns"));
        assertEquals(org.bukkit.damage.DamageEffect.FREEZING, BRIDGE.getDamageEffect("freezing"));
        assertEquals(org.bukkit.damage.DamageEffect.POKING, BRIDGE.getDamageEffect("poking"));
        // Константы Paper берут тот же кэш, что и повторный вызов.
        assertEquals(io.papermc.paper.entity.poi.PoiType.Occupancy.IS_OCCUPIED,
                BRIDGE.createOccupancy("IS_OCCUPIED"));
        assertEquals(io.papermc.paper.entity.poi.PoiType.Occupancy.ANY,
                BRIDGE.createOccupancy("ANY"));
    }

    @Test
    void categoryTableCoversVanillaAndHasNoStaleNames() {
        // 160 записей из {@code EntityTypes} ванильного ядра 26.3.
        assertEquals(160, EntityCategories.size());
        for (String name : EntityCategories.names()) {
            // Имя из ванилы обязано существовать в Bukkit API.
            EntityType.valueOf(name);
        }
    }
}
