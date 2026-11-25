package dev.chunkwake.testplugin;

import java.util.List;
import java.util.logging.Level;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.entity.Entity;
import org.bukkit.entity.EntityType;
import org.bukkit.entity.boat.OakBoat;
import org.bukkit.plugin.java.JavaPlugin;

/**
 * Живая проверка {@code World.spawnEntity}/{@code World.spawn} для немобовых
 * сущностей: появление, видимость в {@code getEntities}/{@code getEntity}/
 * {@code getEntitiesByClass}, удаление и отказ для неподдерживаемых видов.
 * Включается переменной окружения {@code CHUNKWAKE_NONMOB_PROBE}.
 */
final class NonMobProbe {
    private NonMobProbe() {
    }

    static void run(JavaPlugin plugin) {
        World world = plugin.getServer().getWorlds().get(0);
        Location base = world.getSpawnLocation().add(0.5, 20.0, 0.5);
        List<EntityType> types = List.of(
                EntityType.ARROW,
                EntityType.TRIDENT,
                EntityType.TNT,
                EntityType.EXPERIENCE_ORB,
                EntityType.AREA_EFFECT_CLOUD,
                EntityType.EYE_OF_ENDER,
                EntityType.OAK_BOAT,
                EntityType.OAK_CHEST_BOAT,
                EntityType.BAMBOO_RAFT,
                EntityType.MINECART,
                EntityType.CHEST_MINECART,
                EntityType.HOPPER_MINECART,
                EntityType.FURNACE_MINECART,
                EntityType.TNT_MINECART);
        int ok = 0;
        int bad = 0;
        for (EntityType type : types) {
            try {
                Location at = base.clone().add(ok % 5, 0, ok / 5);
                check(plugin, world, type, world.spawnEntity(at, type));
                ok++;
                plugin.getLogger().info("NONMOB ok " + type);
            } catch (Throwable t) {
                bad++;
                plugin.getLogger().log(Level.WARNING, "NONMOB fail " + type, t);
            }
        }
        try {
            OakBoat boat = world.spawn(base, OakBoat.class);
            check(plugin, world, EntityType.OAK_BOAT, boat);
            boat.remove();
            ok++;
            plugin.getLogger().info("NONMOB ok spawn(OakBoat.class)");
        } catch (Throwable t) {
            bad++;
            plugin.getLogger().log(Level.WARNING, "NONMOB fail spawn(OakBoat.class)", t);
        }
        try {
            world.spawnEntity(base, EntityType.ARMOR_STAND);
            bad++;
            plugin.getLogger().warning("NONMOB fail ARMOR_STAND did not throw");
        } catch (UnsupportedOperationException expected) {
            ok++;
            plugin.getLogger().info("NONMOB ok ARMOR_STAND rejected");
        }
        plugin.getLogger().info("NONMOB done ok=" + ok + " bad=" + bad);
    }

    @SuppressWarnings({"rawtypes", "unchecked"})
    private static void check(JavaPlugin plugin, World world, EntityType expected, Entity entity) {
        if (entity == null || entity.getType() != expected) {
            throw new IllegalStateException("wrong entity: " + entity);
        }
        if (world.getEntity(entity.getUniqueId()) == null) {
            throw new IllegalStateException("getEntity(uuid) does not find " + expected);
        }
        boolean listed = world.getEntities().stream()
                .anyMatch(e -> e.getUniqueId().equals(entity.getUniqueId()));
        if (!listed) {
            throw new IllegalStateException("getEntities does not list " + expected);
        }
        Class<Entity> cls = (Class<Entity>) (Class<?>) expected.getEntityClass();
        boolean byClass = world.getEntitiesByClass(cls).stream()
                .anyMatch(e -> e.getUniqueId().equals(entity.getUniqueId()));
        if (!byClass) {
            throw new IllegalStateException("getEntitiesByClass does not list " + expected);
        }
        if (!entity.isValid() || entity.isDead()) {
            throw new IllegalStateException("not valid right after spawn: " + expected);
        }
        plugin.getLogger().info("NONMOB state " + expected + " " + entity.getLocation()
                + " uuid=" + entity.getUniqueId() + " id=" + entity.getEntityId());
        entity.remove();
        if (entity.isValid() || !entity.isDead()) {
            throw new IllegalStateException("still valid after remove: " + expected);
        }
        if (world.getEntity(entity.getUniqueId()) != null) {
            throw new IllegalStateException("getEntity(uuid) still finds removed " + expected);
        }
    }
}
