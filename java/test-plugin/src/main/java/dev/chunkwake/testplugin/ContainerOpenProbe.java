package dev.chunkwake.testplugin;

import org.bukkit.Material;
import org.bukkit.event.EventHandler;
import org.bukkit.event.EventPriority;
import org.bukkit.event.Listener;
import org.bukkit.event.inventory.InventoryOpenEvent;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.ItemStack;
import org.bukkit.plugin.java.JavaPlugin;

/**
 * Живая проба настоящего содержимого в {@code InventoryOpenEvent}:
 * первый открытый сундук пуст, плагин кладёт предметы; последующие
 * открытия должны видеть их (запись в block entity) и перестановку.
 * Включается {@code CHUNKWAKE_CONTAINER_OPEN_PROBE=1}.
 */
final class ContainerOpenProbe implements Listener {
    private static final int X = 8;
    private static final int Y = -60;
    private static final int Z = 10;

    private final JavaPlugin plugin;
    private int opens;
    private int closes;
    private int ok;
    private int bad;

    ContainerOpenProbe(JavaPlugin plugin) {
        this.plugin = plugin;
    }

    @EventHandler
    public void onJoin(PlayerJoinEvent event) {
        var world = event.getPlayer().getWorld();
        plugin.getServer().getScheduler().runTask(plugin, () -> {
            var block = world.getBlockAt(X, Y, Z);
            if (block.getType() != Material.CHEST) {
                block.setType(Material.CHEST);
            }
            plugin.getLogger().info("CONTAINER_OPEN placed " + block.getType()
                    + " at " + block.getLocation() + " player=" + event.getPlayer().getLocation()
                    + " reach=" + event.getPlayer().getLocation().distance(block.getLocation()));
        });
    }

    @EventHandler(priority = EventPriority.MONITOR)
    public void onOpen(InventoryOpenEvent event) {
        if (event.getInventory().getType() != org.bukkit.event.inventory.InventoryType.CHEST) {
            return;
        }
        var location = event.getInventory().getLocation();
        if (location != null && (location.getBlockX() != X
                || location.getBlockY() != Y || location.getBlockZ() != Z)) {
            return;
        }
        opens++;
        try {
            check(location != null, "location");
            Inventory top = event.getInventory();
            var holder = top.getHolder();
            check(holder instanceof org.bukkit.block.Chest, "holder-chest");
            check(holder != null && holder.getInventory() == top, "holder-inventory");
            ItemStack first = top.getItem(0);
            ItemStack second = top.getItem(1);
            if (opens == 1) {
                check(first == null, "first-empty-0");
                check(second == null, "first-empty-1");
                ItemStack dirt = new ItemStack(Material.DIRT, 2);
                var meta = dirt.getItemMeta();
                meta.setCustomModelData(42);
                meta.setUnbreakable(true);
                dirt.setItemMeta(meta);
                ItemStack[] contents = top.getContents();
                contents[0] = dirt;
                contents[1] = new ItemStack(Material.STONE, 3);
                top.setContents(contents);
                check(top.getItem(0) != null && top.getItem(0).getType() == Material.DIRT,
                        "first-set-0");
                Inventory temp = plugin.getServer().createInventory(null, 54);
                temp.addItem(new ItemStack(Material.DIRT, 2));
                check(temp.getContents()[0] != null, "temp-inventory");
                plugin.getLogger().info("CONTAINER_OPEN first size=" + top.getSize());
            } else {
                // Close-sort (ChestSort по умолчанию) переставил предметы.
                check(first != null && first.getType() == Material.STONE
                        && first.getAmount() == 3, "reopen-stone");
                check(second != null && second.getType() == Material.DIRT
                        && second.getAmount() == 2, "reopen-dirt");
                var meta = second == null ? null : second.getItemMeta();
                check(meta != null && meta.hasCustomModelData()
                        && meta.getCustomModelData() == 42, "reopen-cmd");
                check(meta != null && meta.isUnbreakable(), "reopen-unbreakable");
            }
        } catch (Throwable t) {
            bad++;
            plugin.getLogger().log(java.util.logging.Level.WARNING, "CONTAINER_OPEN error", t);
        } finally {
            plugin.getLogger().info(
                    "CONTAINER_OPEN event opens=" + opens + " ok=" + ok + " bad=" + bad);
        }
    }

    @EventHandler(priority = EventPriority.MONITOR)
    public void onClose(org.bukkit.event.inventory.InventoryCloseEvent event) {
        if (event.getInventory().getType() != org.bukkit.event.inventory.InventoryType.CHEST) {
            return;
        }
        var location = event.getInventory().getLocation();
        if (location != null && (location.getBlockX() != X
                || location.getBlockY() != Y || location.getBlockZ() != Z)) {
            return;
        }
        closes++;
        try {
            check(location != null, "close-location");
            Inventory top = event.getInventory();
            check(top.getHolder() instanceof org.bukkit.block.Chest, "close-holder-chest");
            ItemStack first = top.getItem(0);
            ItemStack second = top.getItem(1);
            if (closes == 1) {
                check(first != null && first.getType() == Material.DIRT
                        && first.getAmount() == 2, "close-dirt");
                check(second != null && second.getType() == Material.STONE
                        && second.getAmount() == 3, "close-stone");
                var meta = first == null ? null : first.getItemMeta();
                check(meta != null && meta.hasCustomModelData()
                        && meta.getCustomModelData() == 42, "close-cmd");
                check(meta != null && meta.isUnbreakable(), "close-unbreakable");
                // Эмулируем sort-time=close (по умолчанию у ChestSort).
                ItemStack[] contents = top.getContents();
                ItemStack swap = contents[0];
                contents[0] = contents[1];
                contents[1] = swap;
                top.setContents(contents);
                plugin.getLogger().info("CONTAINER_OPEN close sorted");
            } else {
                check(first != null && first.getType() == Material.STONE
                        && first.getAmount() == 3, "next-close-stone");
                check(second != null && second.getType() == Material.DIRT
                        && second.getAmount() == 2, "next-close-dirt");
                var meta = second == null ? null : second.getItemMeta();
                check(meta != null && meta.hasCustomModelData()
                        && meta.getCustomModelData() == 42, "next-close-cmd");
                check(meta != null && meta.isUnbreakable(), "next-close-unbreakable");
            }
        } catch (Throwable t) {
            bad++;
            plugin.getLogger().log(java.util.logging.Level.WARNING, "CONTAINER_OPEN close error", t);
        } finally {
            plugin.getLogger().info(
                    "CONTAINER_OPEN close closes=" + closes + " ok=" + ok + " bad=" + bad);
        }
    }

    private void check(boolean condition, String what) {
        if (condition) {
            ok++;
        } else {
            bad++;
            plugin.getLogger().warning("CONTAINER_OPEN fail " + what);
        }
    }
}
