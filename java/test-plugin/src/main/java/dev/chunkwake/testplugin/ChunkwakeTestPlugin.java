package dev.chunkwake.testplugin;

import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.format.NamedTextColor;
import org.bukkit.Material;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.block.BlockBreakEvent;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.plugin.java.JavaPlugin;

/** Обычный Bukkit-плагин для проверки моста: команды, события, планировщик. */
public final class ChunkwakeTestPlugin extends JavaPlugin implements Listener {
    private long ticks;

    @Override
    public void onEnable() {
        getLogger().info("onEnable on " + getServer().getName() + " " + getServer().getVersion());
        getServer().getPluginManager().registerEvents(this, this);
        getServer().getScheduler().runTaskTimer(this, () -> {
            ticks += 100;
            getLogger().info("scheduler: " + ticks + " ticks");
        }, 100, 100);
    }

    @Override
    public void onDisable() {
        getLogger().info("onDisable");
    }

    @Override
    public boolean onCommand(CommandSender sender, Command command, String label, String[] args) {
        sender.sendMessage(Component.text("Hello, " + sender.getName() + "!", NamedTextColor.GREEN));
        return true;
    }

    @EventHandler
    public void onJoin(PlayerJoinEvent event) {
        event.getPlayer().sendMessage(Component.text("Welcome to chunkwake, " + event.getPlayer().getName(),
                NamedTextColor.GOLD));
    }

    @EventHandler(ignoreCancelled = true)
    public void onBreak(BlockBreakEvent event) {
        if (event.getBlock().getType() == Material.BEDROCK) {
            event.setCancelled(true);
            event.getPlayer().sendMessage(Component.text("Bedrock is protected by chunkwake-test-plugin",
                    NamedTextColor.RED));
        }
    }
}
