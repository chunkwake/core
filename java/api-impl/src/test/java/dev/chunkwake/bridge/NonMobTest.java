package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import org.bukkit.entity.AreaEffectCloud;
import org.bukkit.entity.ArmorStand;
import org.bukkit.entity.Arrow;
import org.bukkit.entity.Boat;
import org.bukkit.entity.ChestBoat;
import org.bukkit.entity.EnderSignal;
import org.bukkit.entity.EntityType;
import org.bukkit.entity.ExperienceOrb;
import org.bukkit.entity.Minecart;
import org.bukkit.entity.TNTPrimed;
import org.bukkit.entity.Trident;
import org.bukkit.entity.boat.BambooChestRaft;
import org.bukkit.entity.boat.OakBoat;
import org.bukkit.entity.boat.OakChestBoat;
import org.bukkit.entity.minecart.ExplosiveMinecart;
import org.bukkit.entity.minecart.HopperMinecart;
import org.bukkit.entity.minecart.PoweredMinecart;
import org.bukkit.entity.minecart.RideableMinecart;
import org.bukkit.entity.minecart.StorageMinecart;
import org.junit.jupiter.api.Test;

/** Мост `World.spawnEntity` к уже симулируемым немобовым сущностям Rust. */
final class NonMobTest {
    @Test
    void supportsEntitiesWithASummonHelper() {
        assertEquals("minecraft:arrow", NonMobImpl.kind(EntityType.ARROW));
        assertEquals("minecraft:trident", NonMobImpl.kind(EntityType.TRIDENT));
        assertEquals("minecraft:tnt", NonMobImpl.kind(EntityType.TNT));
        assertEquals("minecraft:experience_orb", NonMobImpl.kind(EntityType.EXPERIENCE_ORB));
        assertEquals("minecraft:area_effect_cloud", NonMobImpl.kind(EntityType.AREA_EFFECT_CLOUD));
        assertEquals("minecraft:eye_of_ender", NonMobImpl.kind(EntityType.EYE_OF_ENDER));

        for (EntityType type : List.of(
                EntityType.MINECART,
                EntityType.CHEST_MINECART,
                EntityType.HOPPER_MINECART,
                EntityType.FURNACE_MINECART,
                EntityType.TNT_MINECART)) {
            assertTrue(NonMobImpl.supported(type), type.name());
        }
    }

    @Test
    void supportsEveryBoatAndRaftIncludingChestVariants() {
        for (EntityType type : List.of(
                EntityType.OAK_BOAT,
                EntityType.OAK_CHEST_BOAT,
                EntityType.SPRUCE_BOAT,
                EntityType.SPRUCE_CHEST_BOAT,
                EntityType.BAMBOO_RAFT,
                EntityType.BAMBOO_CHEST_RAFT,
                EntityType.PALE_OAK_BOAT,
                EntityType.PALE_OAK_CHEST_BOAT,
                EntityType.POPLAR_BOAT,
                EntityType.POPLAR_CHEST_BOAT)) {
            assertTrue(NonMobImpl.supported(type), type.name());
            assertEquals(type.getKey().toString(), NonMobImpl.kind(type));
        }
    }

    @Test
    void keepsUnsupportedNonMobEntitiesUnsupported() {
        for (EntityType type : List.of(
                EntityType.ARMOR_STAND,
                EntityType.ITEM_FRAME,
                EntityType.GLOW_ITEM_FRAME,
                EntityType.PAINTING,
                EntityType.END_CRYSTAL,
                EntityType.ITEM,
                EntityType.FALLING_BLOCK,
                EntityType.LIGHTNING_BOLT,
                EntityType.SPECTRAL_ARROW,
                EntityType.SNOWBALL,
                EntityType.COMMAND_BLOCK_MINECART,
                EntityType.SPAWNER_MINECART,
                EntityType.VILLAGER,
                EntityType.INTERACTION,
                EntityType.ITEM_DISPLAY)) {
            assertFalse(NonMobImpl.supported(type), type.name());
            assertNull(NonMobImpl.kind(type), type.name());
        }
        assertNull(NonMobImpl.type(ArmorStand.class));
    }

    @Test
    void mapsExactBukkitInterfaces() {
        assertEquals(EntityType.ARROW, NonMobImpl.type(Arrow.class));
        assertEquals(EntityType.TRIDENT, NonMobImpl.type(Trident.class));
        assertEquals(EntityType.TNT, NonMobImpl.type(TNTPrimed.class));
        assertEquals(EntityType.EXPERIENCE_ORB, NonMobImpl.type(ExperienceOrb.class));
        assertEquals(EntityType.AREA_EFFECT_CLOUD, NonMobImpl.type(AreaEffectCloud.class));
        assertEquals(EntityType.EYE_OF_ENDER, NonMobImpl.type(EnderSignal.class));
        assertEquals(EntityType.OAK_BOAT, NonMobImpl.type(OakBoat.class));
        assertEquals(EntityType.OAK_CHEST_BOAT, NonMobImpl.type(OakChestBoat.class));
        assertEquals(EntityType.BAMBOO_CHEST_RAFT, NonMobImpl.type(BambooChestRaft.class));
        assertEquals(EntityType.MINECART, NonMobImpl.type(RideableMinecart.class));
        assertEquals(EntityType.CHEST_MINECART, NonMobImpl.type(StorageMinecart.class));
        assertEquals(EntityType.HOPPER_MINECART, NonMobImpl.type(HopperMinecart.class));
        assertEquals(EntityType.FURNACE_MINECART, NonMobImpl.type(PoweredMinecart.class));
        assertEquals(EntityType.TNT_MINECART, NonMobImpl.type(ExplosiveMinecart.class));
    }

    @Test
    void mapsGenericBoatAndMinecartClassesToVanillaDefaults() {
        assertEquals(EntityType.OAK_BOAT, NonMobImpl.type(Boat.class));
        assertEquals(EntityType.OAK_CHEST_BOAT, NonMobImpl.type(ChestBoat.class));
        assertEquals(EntityType.MINECART, NonMobImpl.type(Minecart.class));
    }

    @Test
    void parsesStateLineWithLeadingEntityId() {
        org.bukkit.Location l = NonMobImpl.location(null,
                ("7 minecraft:arrow 00000000-0000-0000-0000-000100000000"
                        + " 1.5 -60.25 2.75 90.0 -10.0").split(" "));
        assertEquals(1.5, l.getX(), 1e-9);
        assertEquals(-60.25, l.getY(), 1e-9);
        assertEquals(2.75, l.getZ(), 1e-9);
        assertEquals(90.0f, l.getYaw());
        assertEquals(-10.0f, l.getPitch());
        assertEquals(0.0, NonMobImpl.location(null, "garbage".split(" ")).getX(), 0.0);
        assertEquals(0.0, NonMobImpl.location(null,
                "7 minecraft:arrow 00000000-0000-0000-0000-000100000000"
                        .concat(" not-a-number 2 3 4 5").split(" ")).getX(), 0.0);
    }

    @Test
    void everySupportedTypeHasAnEntityClass() {
        for (EntityType type : EntityType.values()) {
            if (NonMobImpl.supported(type)) {
                assertNotNull(type.getEntityClass(), type.name());
            }
        }
    }
}
