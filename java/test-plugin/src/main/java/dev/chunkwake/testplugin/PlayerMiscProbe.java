package dev.chunkwake.testplugin;

import java.util.List;
import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.block.Block;
import org.bukkit.entity.Arrow;
import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.event.player.PlayerTeleportEvent;
import org.bukkit.inventory.ItemStack;
import org.bukkit.plugin.java.JavaPlugin;
import org.bukkit.util.Vector;

/**
 * Живая проба бытовых API: {@code Player.updateInventory}/performCommand/
 * getNearbyEntities/teleportAsync, legacy-руки и броня, Block.getChunk/
 * getBiome/isPassable/isLiquid/getDrops/breakNaturally, World.getGameTime/
 * getChunkAtAsync. Включается {@code CHUNKWAKE_PLAYER_MISC_PROBE}.
 */
final class PlayerMiscProbe implements Listener {
    private final JavaPlugin plugin;
    private int ok;
    private int bad;

    PlayerMiscProbe(JavaPlugin plugin) {
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
            plugin.getLogger().info("PLAYERMISC ok " + what);
        } else {
            bad++;
            plugin.getLogger().warning("PLAYERMISC fail " + what);
        }
    }

    private void run(Player player) {
        try {
            World world = player.getWorld();
            Location at = player.getLocation();
            check(player.performCommand("list"), "performCommand");
            check(world.getGameTime() >= 0L, "getGameTime");
            check(world.getChunkAtAsync(at).join() != null, "getChunkAtAsync-location");
            check(world.getChunkAtAsync(at.getBlockX() >> 4, at.getBlockZ() >> 4).join() != null,
                    "getChunkAtAsync-coords");

            Block block = world.getBlockAt(at.getBlockX() + 2, at.getBlockY() - 1,
                    at.getBlockZ());
            block.setType(Material.STONE, false);
            check(block.getChunk() != null, "block.getChunk");
            check(!block.isPassable(), "block.isPassable");
            check(!block.isLiquid(), "block.isLiquid");
            check(block.getBiome() != null, "block.getBiome");
            check(!block.getDrops().isEmpty(), "block.getDrops");
            check(block.breakNaturally(), "block.breakNaturally");
            check(block.getType() == Material.AIR, "block.breakNaturally-air");

            player.getInventory().setItemInMainHand(new ItemStack(Material.STONE, 4));
            player.getInventory().setArmorContents(
                    new ItemStack[] {new ItemStack(Material.LEATHER_BOOTS)});
            player.updateInventory();
            check(true, "updateInventory");
        } catch (Throwable t) {
            bad++;
            plugin.getLogger().log(java.util.logging.Level.WARNING, "PLAYERMISC error", t);
            plugin.getServer().getScheduler().runTaskLater(plugin, () -> finish(player), 20L);
            return;
        }
        // Инвентарь применяется контролем асинхронно — проверяем на следующем тике.
        plugin.getServer().getScheduler().runTaskLater(plugin, () -> finish(player), 20L);
    }

    private void finish(Player player) {
        try {
            check(player.getInventory().getItemInHand().getType() == Material.STONE,
                    "getItemInHand");
            check(player.getInventory().getBoots().getType() == Material.LEATHER_BOOTS,
                    "setArmorContents");
            Location at = player.getLocation();
            Arrow arrow = player.getWorld().spawnArrow(at.clone().add(4, 2, 0),
                    new Vector(1, 0, 0), 1.0f, 0.0f);
            List<org.bukkit.entity.Entity> nearby = player.getNearbyEntities(8, 8, 8);
            check(nearby.stream().anyMatch(e -> e.getUniqueId().equals(arrow.getUniqueId())),
                    "getNearbyEntities");
            arrow.remove();
            check(player.teleportAsync(at, PlayerTeleportEvent.TeleportCause.PLUGIN).join(),
                    "teleportAsync");
        } catch (Throwable t) {
            bad++;
            plugin.getLogger().log(java.util.logging.Level.WARNING, "PLAYERMISC finish error", t);
        } finally {
            plugin.getLogger().info("PLAYERMISC done ok=" + ok + " bad=" + bad);
        }
    }
}
