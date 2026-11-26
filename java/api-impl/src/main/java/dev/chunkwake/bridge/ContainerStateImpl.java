package dev.chunkwake.bridge;

import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.block.Block;
import org.bukkit.block.BlockState;
import org.bukkit.event.inventory.InventoryType;
import org.bukkit.inventory.BrewerInventory;
import org.bukkit.inventory.FurnaceInventory;
import org.bukkit.inventory.Inventory;

/**
 * Block state контейнера: {@code Chest}, {@code Barrel}, {@code Furnace} и т. п.
 * {@code getInventory()} читает живой block entity; {@code update()} не нужен,
 * потому что записи применяются сразу.
 */
final class ContainerStateImpl {
    final BlockState api;
    private final Block block;
    private final Material material;
    private final InventoryType type;
    private ContainerInventory inventory;

    private ContainerStateImpl(Block block, Material material, Class<?> iface,
            InventoryType type) {
        this.block = block;
        this.material = material;
        this.type = type;
        this.api = proxy(iface, this);
    }

    @SuppressWarnings({"rawtypes", "unchecked"})
    private static BlockState proxy(Class iface, Object impl) {
        return (BlockState) Stub.of((Class) iface, impl);
    }

    /** {@code null}, если блок не контейнер. */
    static BlockState of(Block block) {
        Material material = block.getType();
        Class<?> iface = interfaceFor(material);
        if (iface == null) {
            return null;
        }
        return new ContainerStateImpl(block, material, iface, typeFor(material)).api;
    }

    static boolean supports(Material material) {
        return interfaceFor(material) != null;
    }

    private static Class<?> interfaceFor(Material material) {
        String key = material.getKey().getKey();
        if (key.equals("barrel")) {
            return org.bukkit.block.Barrel.class;
        }
        if (key.endsWith("shulker_box")) {
            return org.bukkit.block.ShulkerBox.class;
        }
        if (key.equals("furnace")) {
            return org.bukkit.block.Furnace.class;
        }
        if (key.equals("blast_furnace")) {
            return org.bukkit.block.BlastFurnace.class;
        }
        if (key.equals("smoker")) {
            return org.bukkit.block.Smoker.class;
        }
        if (key.equals("hopper")) {
            return org.bukkit.block.Hopper.class;
        }
        if (key.equals("dispenser")) {
            return org.bukkit.block.Dispenser.class;
        }
        if (key.equals("dropper")) {
            return org.bukkit.block.Dropper.class;
        }
        if (key.equals("brewing_stand")) {
            return org.bukkit.block.BrewingStand.class;
        }
        if (key.equals("crafter")) {
            return org.bukkit.block.Crafter.class;
        }
        if (key.equals("chest") || key.equals("trapped_chest")
                || key.endsWith("copper_chest")) {
            return org.bukkit.block.Chest.class;
        }
        return null;
    }

    private static InventoryType typeFor(Material material) {
        String key = material.getKey().getKey();
        return switch (key) {
            case "barrel" -> InventoryType.BARREL;
            case "furnace" -> InventoryType.FURNACE;
            case "blast_furnace" -> InventoryType.BLAST_FURNACE;
            case "smoker" -> InventoryType.SMOKER;
            case "hopper" -> InventoryType.HOPPER;
            case "dispenser" -> InventoryType.DISPENSER;
            case "dropper" -> InventoryType.DROPPER;
            case "brewing_stand" -> InventoryType.BREWING;
            case "crafter" -> InventoryType.CRAFTER;
            default -> key.endsWith("shulker_box")
                    ? InventoryType.SHULKER_BOX : InventoryType.CHEST;
        };
    }

    private static Class<?> inventoryInterface(InventoryType type) {
        return switch (type) {
            case FURNACE, BLAST_FURNACE, SMOKER -> FurnaceInventory.class;
            case BREWING -> BrewerInventory.class;
            default -> Inventory.class;
        };
    }

    private ContainerInventory inventory() {
        if (inventory == null) {
            inventory = new ContainerInventory(block, this, type,
                    inventoryInterface(type));
        }
        return inventory;
    }

    public Inventory getInventory() { return inventory().api; }
    public Inventory getSnapshotInventory() { return inventory().api; }
    public Inventory getBlockInventory() { return inventory().api; }

    public Block getBlock() { return block; }
    public Material getType() { return material; }
    public World getWorld() { return block.getWorld(); }
    public org.bukkit.Location getLocation() { return block.getLocation(); }
    public int getX() { return block.getX(); }
    public int getY() { return block.getY(); }
    public int getZ() { return block.getZ(); }
    public org.bukkit.Chunk getChunk() { return block.getChunk(); }
    public org.bukkit.block.data.BlockData getBlockData() { return block.getBlockData(); }
    public byte getLightLevel() { return block.getLightLevel(); }
    public boolean isPlaced() { return block.getType() == material; }
    public boolean update() { return isPlaced(); }
    public boolean update(boolean applyPhysics) { return update(); }
    public boolean update(boolean applyPhysics, boolean allowSnapshot) { return update(); }
    public BlockState getSnapshot() { return api; }
    public BlockState copy() { return api; }

    public void setType(Material type) { block.setType(type); }
    public void setBlockData(org.bukkit.block.data.BlockData data) { block.setBlockData(data); }
}
