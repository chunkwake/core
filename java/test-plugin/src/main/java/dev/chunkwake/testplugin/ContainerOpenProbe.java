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
    private int clicks;
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

            // Container/BlockState API: getState().getInventory() читает/пишет block entity.
            var chestBlock = world.getBlockAt(X + 2, Y, Z);
            chestBlock.setType(Material.CHEST);
            var chestState = chestBlock.getState();
            check(chestState instanceof org.bukkit.block.Chest, "state-chest");
            if (chestState instanceof org.bukkit.block.Container container) {
                var inv = container.getInventory();
                check(inv.getSize() == 27, "state-size");
                check(inv.getHolder() instanceof org.bukkit.block.Chest, "state-holder");
                check(inv.getLocation().equals(chestBlock.getLocation()), "state-location");
                inv.setItem(0, new ItemStack(Material.DIAMOND, 5));
                var got = inv.getItem(0);
                check(got != null && got.getType() == Material.DIAMOND
                        && got.getAmount() == 5, "state-roundtrip");
                inv.clear(0);
            }

            var barrelBlock = world.getBlockAt(X + 2, Y, Z + 2);
            barrelBlock.setType(Material.BARREL);
            var barrelState = barrelBlock.getState();
            check(barrelState instanceof org.bukkit.block.Barrel, "state-barrel");
            if (barrelState instanceof org.bukkit.block.Container container) {
                var inv = container.getInventory();
                check(inv.getSize() == 27, "state-barrel-size");
                inv.setItem(1, new ItemStack(Material.APPLE, 4));
                var got = inv.getItem(1);
                check(got != null && got.getType() == Material.APPLE
                        && got.getAmount() == 4, "state-barrel-roundtrip");
                inv.clear(1);
            }

            var furnaceBlock = world.getBlockAt(X + 2, Y, Z - 2);
            furnaceBlock.setType(Material.FURNACE);
            var furnaceState = furnaceBlock.getState();
            check(furnaceState instanceof org.bukkit.block.Furnace, "state-furnace");
            if (furnaceState instanceof org.bukkit.block.Furnace furnace) {
                var inv = furnace.getInventory();
                check(inv instanceof org.bukkit.inventory.FurnaceInventory,
                        "state-furnace-inventory");
                inv.setSmelting(new ItemStack(Material.IRON_ORE, 3));
                var got = inv.getSmelting();
                check(got != null && got.getType() == Material.IRON_ORE
                        && got.getAmount() == 3, "state-furnace-roundtrip");
                inv.setSmelting(null);
            }
            plugin.getLogger().info(
                    "CONTAINER_OPEN state ok=" + ok + " bad=" + bad);
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
                // Close-sort (ChestSort по умолчанию) вернул DIRT/STONE на место.
                check(first != null && first.getType() == Material.DIRT
                        && first.getAmount() == 2, "reopen-dirt");
                check(second != null && second.getType() == Material.STONE
                        && second.getAmount() == 3, "reopen-stone");
                var meta = first == null ? null : first.getItemMeta();
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

    @EventHandler(priority = EventPriority.HIGHEST)
    public void onClick(org.bukkit.event.inventory.InventoryClickEvent event) {
        if (event.getInventory().getType() != org.bukkit.event.inventory.InventoryType.CHEST) {
            return;
        }
        var location = event.getInventory().getLocation();
        if (location != null && (location.getBlockX() != X
                || location.getBlockY() != Y || location.getBlockZ() != Z)) {
            return;
        }
        if (clicks > 0) {
            return;
        }
        clicks++;
        try {
            Inventory top = event.getInventory();
            check(event.getClickedInventory() == top, "click-top");
            ItemStack first = top.getItem(0);
            ItemStack second = top.getItem(1);
            check(first != null && first.getType() == Material.DIRT
                    && first.getAmount() == 2, "click-dirt");
            check(second != null && second.getType() == Material.STONE
                    && second.getAmount() == 3, "click-stone");
            var meta = first == null ? null : first.getItemMeta();
            check(meta != null && meta.hasCustomModelData()
                    && meta.getCustomModelData() == 42, "click-cmd");
            check(meta != null && meta.isUnbreakable(), "click-unbreakable");
            check(event.getCurrentItem() != null
                    && event.getCurrentItem().getType() == Material.DIRT, "click-current");
            check(event.getCursor() == null, "click-cursor");
            // Эмулируем hotkey-сортировку: отменяем клик и переставляем слоты.
            event.setCancelled(true);
            ItemStack[] contents = top.getContents();
            ItemStack swap = contents[0];
            contents[0] = contents[1];
            contents[1] = swap;
            top.setContents(contents);
            plugin.getLogger().info("CONTAINER_OPEN click sorted");
        } catch (Throwable t) {
            bad++;
            plugin.getLogger().log(java.util.logging.Level.WARNING, "CONTAINER_OPEN click error", t);
        } finally {
            plugin.getLogger().info(
                    "CONTAINER_OPEN click clicks=" + clicks + " ok=" + ok + " bad=" + bad);
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
                if (clicks > 0) {
                    // Click-sort (hotkey) уже переставил DIRT/STONE.
                    check(first != null && first.getType() == Material.STONE
                            && first.getAmount() == 3, "close-click-stone");
                    check(second != null && second.getType() == Material.DIRT
                            && second.getAmount() == 2, "close-click-dirt");
                    var meta = second == null ? null : second.getItemMeta();
                    check(meta != null && meta.hasCustomModelData()
                            && meta.getCustomModelData() == 42, "close-click-cmd");
                    check(meta != null && meta.isUnbreakable(), "close-click-unbreakable");
                } else {
                    check(first != null && first.getType() == Material.DIRT
                            && first.getAmount() == 2, "close-dirt");
                    check(second != null && second.getType() == Material.STONE
                            && second.getAmount() == 3, "close-stone");
                    var meta = first == null ? null : first.getItemMeta();
                    check(meta != null && meta.hasCustomModelData()
                            && meta.getCustomModelData() == 42, "close-cmd");
                    check(meta != null && meta.isUnbreakable(), "close-unbreakable");
                }
                // Эмулируем sort-time=close (по умолчанию у ChestSort): вернуть DIRT/STONE.
                ItemStack[] contents = top.getContents();
                ItemStack swap = contents[0];
                contents[0] = contents[1];
                contents[1] = swap;
                top.setContents(contents);
                plugin.getLogger().info("CONTAINER_OPEN close sorted");
            } else {
                check(first != null && first.getType() == Material.DIRT
                        && first.getAmount() == 2, "next-close-dirt");
                check(second != null && second.getType() == Material.STONE
                        && second.getAmount() == 3, "next-close-stone");
                var meta = first == null ? null : first.getItemMeta();
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
