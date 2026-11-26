package dev.chunkwake.testplugin;

import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.plugin.java.JavaPlugin;

/**
 * Живая проба видимости: наблюдатель {@code effects} скрывает и показывает
 * игрока {@code target}. Нужны два бота; включается
 * {@code CHUNKWAKE_VISIBILITY_PROBE}.
 */
final class VisibilityProbe implements Listener {
    private final JavaPlugin plugin;
    private int attempts;
    private boolean done;
    private int ok;
    private int bad;

    VisibilityProbe(JavaPlugin plugin) {
        this.plugin = plugin;
    }

    @EventHandler
    public void onJoin(PlayerJoinEvent event) {
        plugin.getServer().getScheduler().runTaskLater(plugin, this::run, 40L);
    }

    private void run() {
        if (done || attempts++ > 20) {
            return;
        }
        Player viewer = plugin.getServer().getPlayerExact("effects");
        Player target = plugin.getServer().getPlayerExact("target");
        if (viewer == null || target == null) {
            plugin.getServer().getScheduler().runTaskLater(plugin, this::run, 20L);
            return;
        }
        done = true;
        try {
            viewer.hidePlayer(plugin, target);
            boolean hidden = !viewer.canSee(target);
            if (hidden) {
                ok++;
                plugin.getLogger().info("VISIBILITY hidden=true target=" + target.getName());
            } else {
                bad++;
                plugin.getLogger().warning("VISIBILITY hidden=false");
            }
        } catch (Throwable t) {
            bad++;
            plugin.getLogger().log(java.util.logging.Level.WARNING, "VISIBILITY hide error", t);
        }
        plugin.getServer().getScheduler().runTaskLater(plugin, () -> {
            try {
                viewer.showPlayer(plugin, target);
                boolean shown = viewer.canSee(target);
                if (shown) {
                    ok++;
                    plugin.getLogger().info("VISIBILITY shown=true");
                } else {
                    bad++;
                    plugin.getLogger().warning("VISIBILITY shown=false");
                }
            } catch (Throwable t) {
                bad++;
                plugin.getLogger().log(java.util.logging.Level.WARNING,
                        "VISIBILITY show error", t);
            } finally {
                plugin.getLogger().info("VISIBILITY done ok=" + ok + " bad=" + bad);
            }
        }, 40L);
    }
}
