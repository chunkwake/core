package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

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
    void combatEntriesKeepDamageAndFallData() {
        var entry = BRIDGE.createCombatEntry(
                (org.bukkit.damage.DamageSource) null, 4.5f,
                io.papermc.paper.world.damagesource.FallLocationType.LADDER, 3.25f);
        assertEquals(4.5f, entry.getDamage());
        assertEquals(io.papermc.paper.world.damagesource.FallLocationType.LADDER,
                entry.getFallLocationType());
        assertEquals(3.25f, entry.getFallDistance());
        assertEquals(null, entry.getDamageSource());
    }

    @Test
    void defaultEntityAttributesMatchVanilla() {
        // Значения из декомпилированного `Zombie.createAttributes`/«Cow».
        var zombie = DefaultAttributeValues.raw("ZOMBIE");
        assertEquals(20.0, zombie.get("max_health"));
        assertEquals(3.0, zombie.get("attack_damage"));
        assertEquals(2.0, zombie.get("armor"));
        assertEquals(35.0, zombie.get("follow_range"));
        assertEquals(0.23, zombie.get("movement_speed"));
        var cow = DefaultAttributeValues.raw("COW");
        assertEquals(10.0, cow.get("max_health"));
        assertEquals(0.2, cow.get("movement_speed"));
        assertEquals(100.0, DefaultAttributeValues.raw("IRON_GOLEM").get("max_health"));
        assertEquals(20.0, DefaultAttributeValues.raw("CREEPER").get("max_health"));
        assertTrue(EntityType.ZOMBIE.hasDefaultAttributes());
        assertFalse(EntityType.ITEM.hasDefaultAttributes());
    }

    @Test
    void defaultAttributeTableHasAllLivingTypes() {
        for (EntityType type : EntityType.values()) {
            if (type == EntityType.UNKNOWN) {
                continue;
            }
            boolean living = type.getEntityClass() != null
                    && org.bukkit.entity.LivingEntity.class.isAssignableFrom(type.getEntityClass());
            assertEquals(living, type.hasDefaultAttributes(), type.name());
        }
    }

    @Test
    void mannequinDefaultsMatchVanilla() {
        // `Mannequin.DEFAULT_PROFILE` = `ResolvableProfile.Static.EMPTY`.
        var profile = BRIDGE.defaultMannequinProfile();
        assertEquals(null, profile.name());
        assertEquals(null, profile.uuid());
        assertTrue(profile.properties().isEmpty());
        assertFalse(profile.dynamic());
        assertNotNull(profile.skinPatch());
        assertTrue(profile.skinPatch().isEmpty());

        // Профиль собирается через ItemComponentTypesBridge (ServiceLoader).
        var built = io.papermc.paper.datacomponent.item.ResolvableProfile.resolvableProfile()
                .name("Notch")
                .uuid(java.util.UUID.fromString("069a79f4-44e9-4726-a5be-fca90e38aaf5"))
                .addProperty(new com.destroystokyo.paper.profile.ProfileProperty("textures", "abc"))
                .build();
        assertEquals("Notch", built.name());
        assertEquals("069a79f4-44e9-4726-a5be-fca90e38aaf5", built.uuid().toString());
        assertEquals(1, built.properties().size());

        // Скин-патч: пустой и собранный.
        var patch = io.papermc.paper.datacomponent.item.ResolvableProfile.SkinPatch.skinPatch()
                .body(net.kyori.adventure.key.Key.key("minecraft", "textures/entity/zombie"))
                .build();
        assertFalse(patch.isEmpty());
        assertEquals("minecraft:textures/entity/zombie", patch.body().asString());
        assertTrue(io.papermc.paper.datacomponent.item.ResolvableProfile.SkinPatch.empty().isEmpty());

        assertEquals(net.kyori.adventure.text.Component.translatable(
                        "entity.minecraft.mannequin.label"),
                BRIDGE.defaultMannequinDescription());

        assertEquals(java.util.Set.of(
                        org.bukkit.entity.Pose.STANDING,
                        org.bukkit.entity.Pose.SNEAKING,
                        org.bukkit.entity.Pose.SWIMMING,
                        org.bukkit.entity.Pose.FALL_FLYING,
                        org.bukkit.entity.Pose.SLEEPING),
                BRIDGE.validMannequinPoses());

        var parts = BRIDGE.allSkinParts();
        assertEquals(0, parts.getRaw());
        assertTrue(parts.hasCapeEnabled());
        assertTrue(parts.hasHatsEnabled());
        parts.setCapeEnabled(false);
        assertFalse(parts.hasCapeEnabled());
        assertEquals(1, parts.getRaw());
        assertTrue(parts.mutableCopy().hasCapeEnabled() == false);
    }

    @Test
    void statisticCriteriaKeysMatchVanillaCustomStats() {
        assertEquals("minecraft.custom:minecraft.deaths",
                BRIDGE.getStatisticCriteriaKey(org.bukkit.Statistic.DEATHS));
        assertEquals("minecraft.custom:minecraft.walk_one_cm",
                BRIDGE.getStatisticCriteriaKey(org.bukkit.Statistic.WALK_ONE_CM));
        assertEquals("minecraft.custom:minecraft.animals_bred",
                BRIDGE.getStatisticCriteriaKey(org.bukkit.Statistic.ANIMALS_BRED));
    }

    @Test
    void legacyMaterialsConvertLikeCraftBukkit() {
        // CraftLegacy.modernize: имя без `LEGACY_`, переименования и data-цвета.
        assertEquals(org.bukkit.Material.STONE,
                LegacyMaterials.modernize(new org.bukkit.material.MaterialData(
                        org.bukkit.Material.LEGACY_STONE), true));
        assertEquals(org.bukkit.Material.SHORT_GRASS,
                LegacyMaterials.modernize(new org.bukkit.material.MaterialData(
                        org.bukkit.Material.LEGACY_GRASS), true));
        assertEquals(org.bukkit.Material.LILY_PAD,
                LegacyMaterials.modernize(new org.bukkit.material.MaterialData(
                        org.bukkit.Material.LEGACY_WATER_LILY), true));
        assertEquals(org.bukkit.Material.OAK_STAIRS,
                LegacyMaterials.modernize(new org.bukkit.material.MaterialData(
                        org.bukkit.Material.LEGACY_WOOD_STAIRS), true));
        assertEquals(org.bukkit.Material.BLACK_WOOL,
                LegacyMaterials.modernize(new org.bukkit.material.MaterialData(
                        org.bukkit.Material.LEGACY_WOOL, (byte) 15), true));
        assertEquals(org.bukkit.Material.RED_STAINED_GLASS,
                LegacyMaterials.modernize(new org.bukkit.material.MaterialData(
                        org.bukkit.Material.LEGACY_STAINED_GLASS, (byte) 14), true));
        assertEquals(org.bukkit.Material.COOKED_SALMON,
                LegacyMaterials.modernize(new org.bukkit.material.MaterialData(
                        org.bukkit.Material.LEGACY_COOKED_FISH, (byte) 1), false));
        // Неизвестный legacy-блок не `null` (иначе `Material.asBlockType` падает).
        assertEquals(org.bukkit.Material.STONE,
                LegacyMaterials.modernize(new org.bukkit.material.MaterialData(
                        org.bukkit.Material.LEGACY_STONE), true));
    }

    @Test
    void persistentDataKeyWrapsKeyAndType() {
        io.papermc.paper.persistence.PersistentDataKey<String> dataKey =
                io.papermc.paper.persistence.PersistentDataKey.of(
                        net.kyori.adventure.key.Key.key("chunkwake", "probe"),
                        org.bukkit.persistence.PersistentDataType.STRING);
        assertEquals("chunkwake:probe", dataKey.key().toString());
        assertEquals(org.bukkit.persistence.PersistentDataType.STRING, dataKey.dataType());
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
