package dev.chunkwake.testplugin;

import org.bukkit.Location;
import org.bukkit.entity.Arrow;
import org.bukkit.entity.ExperienceOrb;
import org.bukkit.entity.Player;
import org.bukkit.entity.Tameable;
import org.bukkit.entity.Wolf;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.plugin.java.JavaPlugin;
import org.bukkit.util.Vector;

/**
 * Живая проба Entity-контракта: Projectile.getShooter/setShooter,
 * Tameable и ExperienceOrb. Включается {@code CHUNKWAKE_ENTITY_CONTRACT_PROBE=1}.
 */
final class EntityContractProbe implements Listener {
    private final JavaPlugin plugin;
    private int ok;
    private int bad;

    EntityContractProbe(JavaPlugin plugin) {
        this.plugin = plugin;
    }

    @EventHandler
    public void onJoin(PlayerJoinEvent event) {
        Player player = event.getPlayer();
        plugin.getServer().getScheduler().runTaskLater(plugin, () -> run(player), 40L);
    }

    private void run(Player player) {
        Location base = player.getLocation().clone().add(2, 0, 0);
        Wolf wolf = null;
        Arrow arrow = null;
        ExperienceOrb orb = null;
        try {
            wolf = player.getWorld().spawn(base, Wolf.class);
            check(wolf instanceof Tameable, "tameable-api");
            Tameable tameable = (Tameable) wolf;
            check(!tameable.isTamed(), "tameable-initial");
            tameable.setOwner(player);
            check(tameable.isTamed(), "tameable-tamed");
            check(player.getUniqueId().equals(tameable.getOwnerUniqueId()),
                    "tameable-owner-id");
            check(tameable.getOwner() != null
                    && player.getUniqueId().equals(tameable.getOwner().getUniqueId()),
                    "tameable-owner");
            wolf.setSitting(true);
            check(wolf.isSitting(), "wolf-sitting");
            wolf.setSitting(false);
            tameable.setTamed(false);
            check(!tameable.isTamed() && tameable.getOwner() == null, "tameable-clear");
            tameable.setTamed(true);
            check(tameable.isTamed() && tameable.getOwner() == null, "tameable-bare");
            tameable.setOwner(player);
            check(tameable.getOwnerUniqueId() != null, "tameable-reowner");

            arrow = player.getWorld().spawnArrow(base.clone().add(0, 3, 0),
                    new Vector(1, 0, 0), 1.0f, 0.0f);
            check(arrow.getShooter() == null, "arrow-no-shooter");
            arrow.setShooter(player);
            check(arrow.getShooter() == player, "arrow-shooter");
            check(player.getUniqueId().equals(arrow.getOwnerUniqueId()), "arrow-owner-id");
            check(!arrow.hasLeftShooter(), "arrow-left-false");
            arrow.setHasLeftShooter(true);
            check(arrow.hasLeftShooter(), "arrow-left-true");
            arrow.setShooter(null);
            check(arrow.getShooter() == null, "arrow-clear");

            orb = player.getWorld().spawn(base.clone().add(1, 0, 1), ExperienceOrb.class);
            check(orb.getExperience() == 1, "orb-default");
            orb.setExperience(37);
            check(orb.getExperience() == 37, "orb-set");
            orb.setCount(3);
            check(orb.getCount() == 3, "orb-count");
            orb.setExperience(5);
            check(orb.getExperience() == 5 && orb.getCount() == 3, "orb-xp-keeps-count");
            check(orb.getSpawnReason() == ExperienceOrb.SpawnReason.UNKNOWN, "orb-reason");
        } catch (Throwable t) {
            bad++;
            plugin.getLogger().log(java.util.logging.Level.WARNING, "ENTITY_CONTRACT error", t);
        } finally {
            if (orb != null) orb.remove();
            if (arrow != null) arrow.remove();
            if (wolf != null) wolf.remove();
            plugin.getLogger().info(
                    "ENTITY_CONTRACT done ok=" + ok + " bad=" + bad);
        }
    }

    private void check(boolean condition, String what) {
        if (condition) {
            ok++;
            plugin.getLogger().info("ENTITY_CONTRACT ok " + what);
        } else {
            bad++;
            plugin.getLogger().warning("ENTITY_CONTRACT fail " + what);
        }
    }
}
