package dev.chunkwake.bridge;

import java.util.List;
import java.util.Locale;
import java.util.UUID;
import org.bukkit.Location;
import org.bukkit.NamespacedKey;
import org.bukkit.World;
import org.bukkit.block.Block;
import org.bukkit.entity.Item;
import org.bukkit.event.block.BlockCookEvent;
import org.bukkit.event.block.BlockDispenseEvent;
import org.bukkit.event.block.CampfireStartEvent;
import org.bukkit.event.inventory.InventoryMoveItemEvent;
import org.bukkit.event.inventory.InventoryPickupItemEvent;
import org.bukkit.event.inventory.InventoryType;
import org.bukkit.inventory.CampfireRecipe;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.RecipeChoice;
import org.bukkit.plugin.PluginManager;
import org.bukkit.util.Vector;

/** События блоков без игрока: воронки, выбрасыватели, раздатчики. */
final class BlockEvents {
    private BlockEvents() {}

    static String fire(PluginManager plugins, World world, int kind, String arg) {
        return switch (kind) {
            case Bridge.EVENT_MOVE_ITEM -> {
                String[] f = arg.split(" ", 10);
                ItemStack item = StackImpl.parse(f[9]);
                var e = new InventoryMoveItemEvent(inventory(world, f, 0), item, inventory(world, f, 4), f[8].equals("1"));
                plugins.callEvent(e);
                if (e.isCancelled()) {
                    yield "c";
                }
                String now = StackImpl.line(e.getItem());
                yield now == null || now.equals(StackImpl.line(item)) ? "" : "i " + now;
            }
            case Bridge.EVENT_HOPPER_PICKUP -> {
                String[] f = arg.split(" ", 4);
                Location at = new Location(world, Integer.parseInt(f[0]) + 0.5, Integer.parseInt(f[1]) + 1, Integer.parseInt(f[2]) + 0.5);
                Item item = Stub.of(Item.class, new GroundItem(at, StackImpl.parse(f[3])));
                var e = new InventoryPickupItemEvent(inventory(world, new String[] {f[0], f[1], f[2], "minecraft:hopper"}, 0), item);
                plugins.callEvent(e);
                yield e.isCancelled() ? "c" : "";
            }
            case Bridge.EVENT_DISPENSE -> {
                String[] f = arg.split(" ", 7);
                var block = world.getBlockAt(Integer.parseInt(f[0]), Integer.parseInt(f[1]), Integer.parseInt(f[2]));
                var e = new BlockDispenseEvent(block, StackImpl.parse(f[6]),
                        new Vector(Double.parseDouble(f[3]), Double.parseDouble(f[4]), Double.parseDouble(f[5])));
                plugins.callEvent(e);
                yield e.isCancelled() ? "c" : "";
            }
            case Bridge.EVENT_CAMPFIRE_START -> campfireStart(plugins, world, arg);
            case Bridge.EVENT_COOK -> cook(plugins, world, arg);
            default -> "";
        };
    }

    /**
     * {@code x y z}\n{@code id опыт время}\n{@code результат}\n{@code исходная стопка}
     * → время готовки ({@link CampfireStartEvent#getTotalCookTime()}).
     */
    private static String campfireStart(PluginManager plugins, World world, String arg) {
        String[] lines = arg.split("\n", -1);
        if (lines.length < 4) {
            return "";
        }
        Block block = blockAt(world, lines[0]);
        String[] meta = lines[1].split(" ");
        float exp = Float.parseFloat(meta[1]);
        int time = Integer.parseInt(meta[2]);
        ItemStack result = StackImpl.parse(lines[2]);
        ItemStack source = StackImpl.parse(lines[3]);
        CampfireRecipe recipe = recipe(meta[0], result, source, exp, time);
        var e = new CampfireStartEvent(block, source, recipe);
        plugins.callEvent(e);
        return Integer.toString(e.getTotalCookTime());
    }

    /**
     * {@code x y z}\n{@code id опыт время}\n{@code сырьё}\n{@code результат}
     * → {@code c}, {@code e} (пустой результат), {@code i стопка} или пусто.
     */
    private static String cook(PluginManager plugins, World world, String arg) {
        String[] lines = arg.split("\n", -1);
        if (lines.length < 4) {
            return "";
        }
        Block block = blockAt(world, lines[0]);
        String[] meta = lines[1].split(" ");
        ItemStack source = StackImpl.parse(lines[2]);
        ItemStack result = StackImpl.parse(lines[3]);
        CampfireRecipe recipe = meta[0].equals("-")
                ? null
                : recipe(meta[0], result, source, Float.parseFloat(meta[1]), Integer.parseInt(meta[2]));
        var e = new BlockCookEvent(block, source, result, recipe);
        plugins.callEvent(e);
        if (e.isCancelled()) {
            return "c";
        }
        String now = StackImpl.line(e.getResult());
        if (now == null) {
            return "e";
        }
        return now.equals(StackImpl.line(result)) ? "" : "i " + now;
    }

    private static Block blockAt(World world, String line) {
        String[] p = line.split(" ");
        return world.getBlockAt(Integer.parseInt(p[0]), Integer.parseInt(p[1]), Integer.parseInt(p[2]));
    }

    private static CampfireRecipe recipe(String id, ItemStack result, ItemStack source, float exp, int time) {
        NamespacedKey key = NamespacedKey.fromString(id);
        if (key == null) {
            key = NamespacedKey.minecraft("campfire_cooking");
        }
        return new CampfireRecipe(key, result, new RecipeChoice.ExactChoice(List.of(source)), exp, time);
    }

    /** Инвентарь блока из полей {@code x y z имя}, начиная с {@code at}. */
    private static Inventory inventory(World world, String[] f, int at) {
        Location loc = new Location(world, Integer.parseInt(f[at]), Integer.parseInt(f[at + 1]), Integer.parseInt(f[at + 2]));
        String name = f[at + 3].substring(f[at + 3].indexOf(':') + 1);
        InventoryType type = switch (name) {
            case "chest", "trapped_chest" -> InventoryType.CHEST;
            default -> {
                try {
                    yield InventoryType.valueOf(name.toUpperCase(Locale.ROOT));
                } catch (IllegalArgumentException e) {
                    yield InventoryType.CHEST;
                }
            }
        };
        return Stub.of(Inventory.class, new BlockInventory(type, loc));
    }

    /** Инвентарь контейнера-блока для событий: тип, размер, место. */
    record BlockInventory(InventoryType type, Location location) {
        public InventoryType getType() { return type; }
        public int getSize() { return type.getDefaultSize(); }
        public Location getLocation() { return location.clone(); }
        public org.bukkit.inventory.InventoryHolder getHolder() { return null; }
        public org.bukkit.inventory.InventoryHolder getHolder(boolean snapshot) { return null; }
        public List<org.bukkit.entity.HumanEntity> getViewers() { return List.of(); }
        public int getMaxStackSize() { return 99; }
        public ItemStack[] getContents() { return new ItemStack[getSize()]; }
        public ItemStack[] getStorageContents() { return getContents(); }
        public ItemStack getItem(int index) { return null; }
        public boolean isEmpty() { return true; }
        public int firstEmpty() { return 0; }
    }

    /** Предмет на земле для {@link InventoryPickupItemEvent}. */
    record GroundItem(Location location, ItemStack stack) {
        public ItemStack getItemStack() { return stack; }
        public UUID getUniqueId() { return new UUID(0, location.hashCode()); }
        public World getWorld() { return location.getWorld(); }
        public Location getLocation() { return location.clone(); }
        public boolean isValid() { return true; }
        public org.bukkit.entity.EntityType getType() { return org.bukkit.entity.EntityType.ITEM; }
    }
}
