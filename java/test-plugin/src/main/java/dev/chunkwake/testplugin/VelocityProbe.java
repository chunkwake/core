package dev.chunkwake.testplugin;

import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.entity.Arrow;
import org.bukkit.entity.Cow;
import org.bukkit.entity.EntityType;
import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.plugin.java.JavaPlugin;
import org.bukkit.util.Vector;

/**
 * Живая проба {@code Entity.getVelocity/setVelocity}: стрела, моб и игрок.
 * Включается {@code CHUNKWAKE_VELOCITY_PROBE}.
 */
final class VelocityProbe implements Listener {
    private final JavaPlugin plugin;
    private int ok;
    private int bad;

    VelocityProbe(JavaPlugin plugin) {
        this.plugin = plugin;
    }

    @EventHandler
    public void onJoin(PlayerJoinEvent event) {
        Player player = event.getPlayer();
        plugin.getServer().getScheduler().runTaskLater(plugin, () -> run(player), 40L);
    }

    private void check(double ax, double ay, double az, Vector got, String what) {
        double d = Math.abs(ax - got.getX()) + Math.abs(ay - got.getY()) + Math.abs(az - got.getZ());
        if (d < 1e-6) {
            ok++;
            plugin.getLogger().info("VELOCITY ok " + what + " " + got);
        } else {
            bad++;
            plugin.getLogger().warning("VELOCITY fail " + what + " expected="
                    + ax + "," + ay + "," + az + " got=" + got);
        }
    }

    private void run(Player player) {
        try {
            World world = player.getWorld();
            Location at = player.getLocation();

            Arrow arrow = world.spawnArrow(at.clone().add(3, 2, 0),
                    new Vector(1, 0, 0), 0.0f, 0.0f);
            arrow.setVelocity(new Vector(0.5, 0.1, -0.25));
            check(0.5, 0.1, -0.25, arrow.getVelocity(), "arrow");
            arrow.remove();

            Cow cow = (Cow) world.spawnEntity(at.clone().add(-3, 0, 0), EntityType.COW);
            cow.setVelocity(new Vector(-0.2, 0.3, 0.4));
            check(-0.2, 0.3, 0.4, cow.getVelocity(), "mob");
            cow.remove();

            player.setVelocity(new Vector(0.1, 0.2, 0.3));
            check(0.1, 0.2, 0.3, player.getVelocity(), "player");
        } catch (Throwable t) {
            bad++;
            plugin.getLogger().log(java.util.logging.Level.WARNING, "VELOCITY error", t);
        } finally {
            plugin.getLogger().info("VELOCITY done ok=" + ok + " bad=" + bad);
        }
    }
}
