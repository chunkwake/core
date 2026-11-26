package dev.chunkwake.testplugin;

import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.entity.Arrow;
import org.bukkit.entity.LightningStrike;
import org.bukkit.entity.Player;
import org.bukkit.entity.Trident;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.event.weather.LightningStrikeEvent;
import org.bukkit.plugin.java.JavaPlugin;
import org.bukkit.util.Vector;

/**
 * Живая проба мировых эффектов: молния/молния-эффект, createExplosion с
 * сохранением и разрушением блоков, spawnArrow (стрела и трезубец).
 * Включается {@code CHUNKWAKE_WORLD_FX_PROBE}; отмена молнии —
 * {@code CHUNKWAKE_WORLD_FX_CANCEL}.
 */
final class WorldFxProbe implements Listener {
    private final JavaPlugin plugin;
    private int ok;
    private int bad;

    WorldFxProbe(JavaPlugin plugin) {
        this.plugin = plugin;
    }

    @EventHandler
    public void onJoin(PlayerJoinEvent event) {
        Player player = event.getPlayer();
        plugin.getServer().getScheduler().runTaskLater(plugin, () -> run(player), 40L);
    }

    @EventHandler
    public void onLightning(LightningStrikeEvent event) {
        boolean cancel = System.getenv("CHUNKWAKE_WORLD_FX_CANCEL") != null;
        plugin.getLogger().info("WORLDFX lightning-event effect="
                + event.getLightning().isEffect() + " cancel=" + cancel);
        if (cancel) {
            event.setCancelled(true);
        }
    }

    private void check(boolean condition, String what) {
        if (condition) {
            ok++;
            plugin.getLogger().info("WORLDFX ok " + what);
        } else {
            bad++;
            plugin.getLogger().warning("WORLDFX fail " + what);
        }
    }

    private void run(Player player) {
        try {
            World world = player.getWorld();
            Location at = player.getLocation();
            LightningStrike struck = world.strikeLightning(at.clone().add(5, 0, 0));
            check(struck != null && struck.getType().name().equals("LIGHTNING_BOLT")
                    && !struck.isEffect() && struck.isValid(), "lightning");
            LightningStrike visual = world.strikeLightningEffect(at.clone().add(6, 0, 0));
            check(visual != null && visual.isEffect() && visual.isValid(), "lightning-effect");

            int keepX = at.getBlockX() + 10;
            int breakX = at.getBlockX() - 10;
            int groundY = at.getBlockY() - 1;
            world.getBlockAt(keepX, groundY, at.getBlockZ()).setType(Material.STONE, false);
            world.getBlockAt(breakX, groundY, at.getBlockZ()).setType(Material.STONE, false);
            world.createExplosion(keepX + 0.5, groundY, at.getBlockZ() + 0.5, 2.0f, false, false);
            world.createExplosion(breakX + 0.5, groundY, at.getBlockZ() + 0.5, 2.0f, false, true);
            check(world.getBlockAt(keepX, groundY, at.getBlockZ()).getType() == Material.STONE,
                    "explosion-keep");
            check(world.getBlockAt(breakX, groundY, at.getBlockZ()).getType() == Material.AIR,
                    "explosion-break");

            Arrow arrow = world.spawnArrow(at.clone().add(-3, 5, 0), new Vector(1, 0, 0), 1.5f, 0.0f);
            boolean arrowFound = world.getEntity(arrow.getUniqueId()) != null;
            plugin.getLogger().info("WORLDFX arrow type=" + arrow.getType()
                    + " uuid=" + arrow.getUniqueId() + " found=" + arrowFound);
            check(arrowFound && !arrow.isDead(), "spawnArrow");
            arrow.remove();

            Trident trident = world.spawnArrow(at.clone().add(3, 5, 0),
                    new Vector(-1, 0, 0), 1.0f, 0.0f, Trident.class);
            boolean tridentFound = world.getEntity(trident.getUniqueId()) != null;
            plugin.getLogger().info("WORLDFX trident type=" + trident.getType()
                    + " uuid=" + trident.getUniqueId() + " found=" + tridentFound);
            check(tridentFound && !trident.isDead(), "spawnArrow-trident");
            trident.remove();
        } catch (Throwable t) {
            bad++;
            plugin.getLogger().log(java.util.logging.Level.WARNING, "WORLDFX error", t);
        } finally {
            plugin.getLogger().info("WORLDFX done ok=" + ok + " bad=" + bad);
        }
    }
}
