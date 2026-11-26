package dev.chunkwake.testplugin;

import net.kyori.adventure.text.Component;
import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.plugin.java.JavaPlugin;

/** Живая проба заголовка/подвала таб-листа; env {@code CHUNKWAKE_TABLIST_PROBE}. */
final class TabListProbe implements Listener {
    private final JavaPlugin plugin;
    private int ok;
    private int bad;

    TabListProbe(JavaPlugin plugin) {
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
            plugin.getLogger().info("TABLIST ok " + what);
        } else {
            bad++;
            plugin.getLogger().warning("TABLIST fail " + what);
        }
    }

    private void run(Player player) {
        try {
            player.setPlayerListHeader("Header");
            check("Header".equals(player.getPlayerListHeader()), "header");
            player.setPlayerListFooter("Footer");
            check("Footer".equals(player.getPlayerListFooter()), "footer");
            player.sendPlayerListHeaderAndFooter(Component.text("H2"), Component.text("F2"));
            check("H2".equals(player.getPlayerListHeader()), "component-header");
            check("F2".equals(player.getPlayerListFooter()), "component-footer");
        } catch (Throwable t) {
            bad++;
            plugin.getLogger().log(java.util.logging.Level.WARNING, "TABLIST error", t);
        } finally {
            plugin.getLogger().info("TABLIST done ok=" + ok + " bad=" + bad);
        }
    }
}
