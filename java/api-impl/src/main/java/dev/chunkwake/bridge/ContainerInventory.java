package dev.chunkwake.bridge;

import java.util.Arrays;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.block.Block;
import org.bukkit.entity.HumanEntity;
import org.bukkit.event.inventory.InventoryType;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.InventoryHolder;
import org.bukkit.inventory.ItemStack;

/**
 * Инвентарь настоящего block entity ({@code Chest/Barrel/Furnace.getInventory()}).
 * Каждый вызов читает/пишет Rust через {@code container ...}; правки сразу попадают
 * в block entity и сохраняются, поэтому следующий open_screen видит их.
 */
final class ContainerInventory {
    final Inventory api;
    private final Block block;
    private final ContainerStateImpl state;
    private final InventoryType type;
    private final int size;
    private int maxStack = 64;

    ContainerInventory(Block block, ContainerStateImpl state, InventoryType type,
            Class<?> iface) {
        this.block = block;
        this.state = state;
        this.type = type;
        this.size = detectSize(type);
        this.api = (Inventory) Stub.of(iface, this);
    }

    private String pos() {
        return block.getX() + " " + block.getY() + " " + block.getZ();
    }

    private String request(String line) {
        return WorldImpl.request(block.getWorld(), line);
    }

    private String requestLarge(String line) {
        return WorldImpl.requestLarge(block.getWorld(), line);
    }

    private static ItemStack parse(String line) {
        return line == null || line.isEmpty() || line.equals("-") ? null : StackImpl.parse(line);
    }

    private static String line(ItemStack stack) {
        String line = StackImpl.line(stack);
        return line == null ? "-" : line;
    }

    private int detectSize(InventoryType type) {
        String data = requestLarge("container slots " + pos());
        if (!data.isEmpty()) {
            int end = data.indexOf('\n');
            try {
                int n = Integer.parseInt((end < 0 ? data : data.substring(0, end)).trim());
                if (n > 0 && n <= 54) {
                    return n;
                }
            } catch (NumberFormatException ignored) {
                // fallback по типу
            }
        }
        int fallback = type.getDefaultSize();
        return fallback > 0 ? fallback : 27;
    }

    private ItemStack[] load() {
        ItemStack[] out = new ItemStack[size];
        String data = requestLarge("container slots " + pos());
        if (data.isEmpty()) {
            return out;
        }
        String[] lines = data.split("\n", -1);
        for (int i = 0; i < size && 1 + i < lines.length; i++) {
            out[i] = parse(lines[1 + i]);
        }
        return out;
    }

    private void store(ItemStack[] contents) {
        StringBuilder out = new StringBuilder("container setall ").append(pos());
        for (int i = 0; i < size; i++) {
            out.append('\n').append(line(contents != null && i < contents.length ? contents[i] : null));
        }
        requestLarge(out.toString());
    }

    public int getSize() { return size; }
    public int getMaxStackSize() { return maxStack; }
    public void setMaxStackSize(int max) { maxStack = Math.max(1, max); }
    public InventoryType getType() { return type; }
    public Location getLocation() { return block.getLocation(); }
    public InventoryHolder getHolder() { return (InventoryHolder) state.api; }
    public InventoryHolder getHolder(boolean useSnapshot) { return (InventoryHolder) state.api; }
    public World getWorld() { return block.getWorld(); }

    public ItemStack getItem(int index) {
        if (index < 0 || index >= size) {
            throw new ArrayIndexOutOfBoundsException(index);
        }
        return parse(request("container get " + pos() + " " + index));
    }

    public void setItem(int index, ItemStack item) {
        if (index < 0 || index >= size) {
            throw new ArrayIndexOutOfBoundsException(index);
        }
        requestLarge("container set " + pos() + " " + index + " " + line(item));
    }

    public ItemStack[] getContents() { return load(); }

    public void setContents(ItemStack[] contents) {
        if (contents != null && contents.length > size) {
            throw new IllegalArgumentException(
                    "Invalid inventory size (" + contents.length + "); expected " + size + " or less");
        }
        store(contents);
    }

    public ItemStack[] getStorageContents() { return getContents(); }
    public void setStorageContents(ItemStack[] contents) { setContents(contents); }

    public boolean isEmpty() {
        for (ItemStack item : getContents()) {
            if (item != null && !item.getType().isAir() && item.getAmount() > 0) {
                return false;
            }
        }
        return true;
    }

    public void clear() { store(new ItemStack[size]); }
    public void clear(int index) { setItem(index, null); }

    /** {@code CraftInventory.addItem}: слияние в существующие стопки, затем пустые слоты. */
    public HashMap<Integer, ItemStack> addItem(ItemStack... items) {
        HashMap<Integer, ItemStack> left = new HashMap<>();
        ItemStack[] contents = getContents();
        Map<Integer, ItemStack> changed = new LinkedHashMap<>();
        for (int k = 0; k < items.length; k++) {
            ItemStack item = items[k];
            if (item == null || item.isEmpty()) {
                continue;
            }
            int rest = item.getAmount();
            int max = Math.min(maxStack, item.getMaxStackSize());
            for (int i = 0; i < contents.length && rest > 0; i++) {
                ItemStack cur = contents[i];
                if (cur != null && cur.isSimilar(item) && cur.getAmount() < max) {
                    int put = Math.min(rest, max - cur.getAmount());
                    cur.setAmount(cur.getAmount() + put);
                    rest -= put;
                    changed.put(i, cur);
                }
            }
            for (int i = 0; i < contents.length && rest > 0; i++) {
                if (contents[i] == null || contents[i].isEmpty()) {
                    ItemStack stack = item.clone();
                    stack.setAmount(Math.min(rest, max));
                    rest -= stack.getAmount();
                    contents[i] = stack;
                    changed.put(i, stack);
                }
            }
            if (rest > 0) {
                ItemStack stack = item.clone();
                stack.setAmount(rest);
                left.put(k, stack);
            }
        }
        for (Map.Entry<Integer, ItemStack> entry : changed.entrySet()) {
            setItem(entry.getKey(), entry.getValue());
        }
        return left;
    }

    public HashMap<Integer, ItemStack> removeItem(ItemStack... items) {
        HashMap<Integer, ItemStack> left = new HashMap<>();
        ItemStack[] contents = getContents();
        Map<Integer, ItemStack> changed = new LinkedHashMap<>();
        for (int k = 0; k < items.length; k++) {
            ItemStack item = items[k];
            if (item == null) {
                continue;
            }
            int rest = item.getAmount();
            for (int i = 0; i < contents.length && rest > 0; i++) {
                ItemStack cur = contents[i];
                if (cur != null && cur.isSimilar(item)) {
                    int take = Math.min(rest, cur.getAmount());
                    rest -= take;
                    cur.setAmount(cur.getAmount() - take);
                    changed.put(i, cur.getAmount() > 0 ? cur : null);
                }
            }
            if (rest > 0) {
                ItemStack stack = item.clone();
                stack.setAmount(rest);
                left.put(k, stack);
            }
        }
        for (Map.Entry<Integer, ItemStack> entry : changed.entrySet()) {
            setItem(entry.getKey(), entry.getValue());
        }
        return left;
    }

    public HashMap<Integer, ItemStack> removeItemAnySlot(ItemStack... items) {
        return removeItem(items);
    }

    public boolean contains(Material material) { return first(material) >= 0; }
    public boolean contains(ItemStack item) { return item != null && first(item) >= 0; }

    public boolean contains(Material material, int amount) {
        return count(material, null) >= amount;
    }

    public boolean contains(ItemStack item, int amount) {
        return item != null && count(null, item) >= amount;
    }

    public boolean containsAtLeast(ItemStack item, int amount) {
        return item != null && count(null, item) >= amount;
    }

    private int count(Material material, ItemStack similar) {
        int total = 0;
        for (ItemStack item : getContents()) {
            if (item == null) {
                continue;
            }
            if (material != null && item.getType() == material) {
                total += item.getAmount();
            } else if (similar != null && item.isSimilar(similar)) {
                total += item.getAmount();
            }
        }
        return total;
    }

    public HashMap<Integer, ItemStack> all(Material material) {
        HashMap<Integer, ItemStack> out = new LinkedHashMap<>();
        ItemStack[] contents = getContents();
        for (int i = 0; i < contents.length; i++) {
            if (contents[i] != null && contents[i].getType() == material) {
                out.put(i, contents[i]);
            }
        }
        return out;
    }

    public HashMap<Integer, ItemStack> all(ItemStack item) {
        HashMap<Integer, ItemStack> out = new LinkedHashMap<>();
        if (item == null) {
            return out;
        }
        ItemStack[] contents = getContents();
        for (int i = 0; i < contents.length; i++) {
            if (contents[i] != null && contents[i].isSimilar(item)) {
                out.put(i, contents[i]);
            }
        }
        return out;
    }

    public int first(Material material) {
        ItemStack[] contents = getContents();
        for (int i = 0; i < contents.length; i++) {
            if (contents[i] != null && contents[i].getType() == material) {
                return i;
            }
        }
        return -1;
    }

    public int first(ItemStack item) {
        if (item == null) {
            return -1;
        }
        ItemStack[] contents = getContents();
        for (int i = 0; i < contents.length; i++) {
            if (contents[i] != null && contents[i].isSimilar(item)) {
                return i;
            }
        }
        return -1;
    }

    public int firstEmpty() {
        ItemStack[] contents = getContents();
        for (int i = 0; i < contents.length; i++) {
            if (contents[i] == null || contents[i].isEmpty()) {
                return i;
            }
        }
        return -1;
    }

    public List<HumanEntity> getViewers() { return List.of(); }
    public int close() { return 0; }

    public java.util.ListIterator<ItemStack> iterator() {
        return Arrays.asList(getContents()).listIterator();
    }

    public java.util.ListIterator<ItemStack> iterator(int index) {
        return Arrays.asList(getContents()).listIterator(index);
    }

    // --- FurnaceInventory ---

    public ItemStack getSmelting() { return getItem(0); }
    public void setSmelting(ItemStack item) { setItem(0, item); }
    public ItemStack getFuel() {
        return getItem(type == InventoryType.BREWING ? 4 : 1);
    }

    public void setFuel(ItemStack item) {
        setItem(type == InventoryType.BREWING ? 4 : 1, item);
    }
    public ItemStack getResult() { return getItem(2); }
    public void setResult(ItemStack item) { setItem(2, item); }
    public boolean isFuel(ItemStack item) { return item != null && !item.getType().isAir(); }
    public boolean canSmelt(ItemStack item) { return item != null && !item.getType().isAir(); }

    // --- BrewerInventory ---

    public ItemStack getIngredient() { return getItem(3); }
    public void setIngredient(ItemStack item) { setItem(3, item); }
}
