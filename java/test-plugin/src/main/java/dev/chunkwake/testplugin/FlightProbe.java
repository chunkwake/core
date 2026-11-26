package dev.chunkwake.testplugin;

import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.plugin.java.JavaPlugin;

/** Живая проба {@code Player.setAllowFlight/setFlying}; env {@code CHUNKWAKE_FLIGHT_PROBE}. */
final class FlightProbe implements Listener {
    private final JavaPlugin plugin;
    private int ok;
    private int bad;

    FlightProbe(JavaPlugin plugin) {
        this.plugin = plugin;
    }

    @EventHandler
    public void onJoin(PlayerJoinEvent event) {
        Player player = event.getPlayer();
        plugin.getServer().getScheduler().runTaskLater(plugin, () -> run(player), 40L);
    }

    private void check(boolean condition, String what) {
        if (condition) {
            ok++;
            plugin.getLogger().info("FLIGHT ok " + what);
        } else {
            bad++;
            plugin.getLogger().warning("FLIGHT fail " + what);
        }
    }

    private void run(Player player) {
        try {
            player.setAllowFlight(true);
            check(player.getAllowFlight(), "allowflight-on");
            player.setFlying(true);
            check(player.isFlying(), "flying-on");
            player.setFlying(false);
            check(!player.isFlying(), "flying-off");
            player.setAllowFlight(false);
            check(!player.getAllowFlight(), "allowflight-off");
        } catch (Throwable t) {
            bad++;
            plugin.getLogger().log(java.util.logging.Level.WARNING, "FLIGHT error", t);
        } finally {
            plugin.getLogger().info("FLIGHT done ok=" + ok + " bad=" + bad);
        }
    }
}
