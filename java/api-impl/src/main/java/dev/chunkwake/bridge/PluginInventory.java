package dev.chunkwake.bridge;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import net.kyori.adventure.text.Component;
import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.entity.HumanEntity;
import org.bukkit.event.inventory.InventoryType;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.InventoryHolder;
import org.bukkit.inventory.ItemStack;

/**
 * {@link Inventory} из {@code Bukkit.createInventory}: слоты хранит Rust
 * (запросы {@code inv new|get|set}), окно открывает {@code Player.openInventory}.
 */
final class PluginInventory {
    private static final Map<Integer, PluginInventory> BY_ID = new ConcurrentHashMap<>();

    final Inventory api = Stub.of(Inventory.class, this);
    final int id;
    final int size;
    final InventoryType type;
    final InventoryHolder holder;
    final Component title;
    final List<HumanEntity> viewers = new ArrayList<>();
    private int maxStack = 99;

    private PluginInventory(int id, int size, InventoryType type, InventoryHolder holder, Component title) {
        this.id = id;
        this.size = size;
        this.type = type;
        this.holder = holder;
        this.title = title;
    }

    static Inventory create(InventoryHolder holder, InventoryType type, int size, Component title) {
        String kind = switch (type) {
            case HOPPER -> "hopper";
            case DISPENSER, DROPPER -> "dispenser";
            case CHEST, ENDER_CHEST, BARREL, SHULKER_BOX -> "chest";
            default -> throw new UnsupportedOperationException("chunkwake: createInventory " + type);
        };
        if (kind.equals("chest") && (size <= 0 || size > 54 || size % 9 != 0)) {
            throw new IllegalArgumentException("Size for custom inventory must be a multiple of 9 between 9 and 54 slots (got " + size + ")");
        }
        String r = Bridge.world("inv new " + size + " " + kind);
        if (r.isEmpty()) {
            throw new IllegalArgumentException("chunkwake: bad inventory " + type + " " + size);
        }
        if (title == null) {
            title = Component.translatable(switch (type) {
                case HOPPER -> "container.hopper";
                case DISPENSER -> "container.dispenser";
                case DROPPER -> "container.dropper";
                case ENDER_CHEST -> "container.enderchest";
                case BARREL -> "container.barrel";
                case SHULKER_BOX -> "container.shulkerBox";
                default -> size == 54 ? "container.chestDouble" : "container.chest";
            });
        }
        PluginInventory inv = new PluginInventory(Integer.parseInt(r.trim()), size, type, holder, title);
        BY_ID.put(inv.id, inv);
        return inv.api;
    }

    static PluginInventory get(int id) {
        return BY_ID.get(id);
    }

    private String[] lines() {
        return Bridge.world("inv get " + id).split("\n", -1);
    }

    private static ItemStack stack(String line) {
        return line.isEmpty() || line.equals("-") ? null : StackImpl.parse(line);
    }

    public int getSize() { return size; }
    public InventoryType getType() { return type; }
    public InventoryHolder getHolder() { return holder; }
    public InventoryHolder getHolder(boolean useSnapshot) { return holder; }
    public List<HumanEntity> getViewers() { return new ArrayList<>(viewers); }
    public int getMaxStackSize() { return maxStack; }
    public void setMaxStackSize(int size) { maxStack = size; }
    public Location getLocation() { return null; }

    public ItemStack getItem(int index) {
        if (index < 0 || index >= size) {
            throw new ArrayIndexOutOfBoundsException(index);
        }
        String[] l = lines();
        return index < l.length ? stack(l[index]) : null;
    }

    public void setItem(int index, ItemStack item) {
        if (index < 0 || index >= size) {
            throw new ArrayIndexOutOfBoundsException(index);
        }
        String line = StackImpl.line(item);
        Bridge.world("inv set " + id + " " + index + " " + (line == null ? "-" : line));
    }

    public ItemStack[] getContents() {
        String[] l = lines();
        ItemStack[] out = new ItemStack[size];
        for (int i = 0; i < size && i < l.length; i++) {
            out[i] = stack(l[i]);
        }
        return out;
    }

    public ItemStack[] getStorageContents() { return getContents(); }

    public void setContents(ItemStack[] items) {
        if (items.length > size) {
            throw new IllegalArgumentException("Invalid inventory size (" + items.length + "); expected " + size + " or less");
        }
        for (int i = 0; i < size; i++) {
            setItem(i, i < items.length ? items[i] : null);
        }
    }

    public void setStorageContents(ItemStack[] items) { setContents(items); }

    public void clear() {
        for (int i = 0; i < size; i++) {
            setItem(i, null);
        }
    }

    public void clear(int index) { setItem(index, null); }

    public boolean isEmpty() {
        return Arrays.stream(getContents()).allMatch(s -> s == null || s.isEmpty());
    }

    public int firstEmpty() {
        ItemStack[] c = getContents();
        for (int i = 0; i < c.length; i++) {
            if (c[i] == null || c[i].isEmpty()) {
                return i;
            }
        }
        return -1;
    }

    public int first(Material m) {
        ItemStack[] c = getContents();
        for (int i = 0; i < c.length; i++) {
            if (c[i] != null && c[i].getType() == m) {
                return i;
            }
        }
        return -1;
    }

    public int first(ItemStack item) {
        ItemStack[] c = getContents();
        for (int i = 0; i < c.length; i++) {
            if (c[i] != null && c[i].equals(item)) {
                return i;
            }
        }
        return -1;
    }

    public boolean contains(Material m) { return first(m) >= 0; }
    public boolean contains(ItemStack item) { return item != null && first(item) >= 0; }

    public boolean containsAtLeast(ItemStack item, int amount) {
        int n = 0;
        for (ItemStack s : getContents()) {
            if (s != null && s.isSimilar(item)) {
                n += s.getAmount();
            }
        }
        return n >= amount;
    }

    public HashMap<Integer, ItemStack> all(Material m) {
        HashMap<Integer, ItemStack> out = new HashMap<>();
        ItemStack[] c = getContents();
        for (int i = 0; i < c.length; i++) {
            if (c[i] != null && c[i].getType() == m) {
                out.put(i, c[i]);
            }
        }
        return out;
    }

    /** Как {@code CraftInventory.addItem}: сперва досыпать похожие, потом в пустые. */
    public HashMap<Integer, ItemStack> addItem(ItemStack... items) {
        HashMap<Integer, ItemStack> left = new HashMap<>();
        ItemStack[] c = getContents();
        for (int k = 0; k < items.length; k++) {
            ItemStack it = items[k];
            if (it == null || it.isEmpty()) {
                continue;
            }
            int rest = it.getAmount();
            int max = Math.min(maxStack, it.getMaxStackSize());
            for (int i = 0; i < c.length && rest > 0; i++) {
                if (c[i] != null && c[i].isSimilar(it) && c[i].getAmount() < max) {
                    int put = Math.min(rest, max - c[i].getAmount());
                    c[i].setAmount(c[i].getAmount() + put);
                    rest -= put;
                    setItem(i, c[i]);
                }
            }
            for (int i = 0; i < c.length && rest > 0; i++) {
                if (c[i] == null || c[i].isEmpty()) {
                    ItemStack s = it.clone();
                    s.setAmount(Math.min(rest, max));
                    rest -= s.getAmount();
                    c[i] = s;
                    setItem(i, s);
                }
            }
            if (rest > 0) {
                ItemStack s = it.clone();
                s.setAmount(rest);
                left.put(k, s);
            }
        }
        return left;
    }

    public HashMap<Integer, ItemStack> removeItem(ItemStack... items) {
        HashMap<Integer, ItemStack> left = new HashMap<>();
        ItemStack[] c = getContents();
        for (int k = 0; k < items.length; k++) {
            ItemStack it = items[k];
            if (it == null) {
                continue;
            }
            int rest = it.getAmount();
            for (int i = 0; i < c.length && rest > 0; i++) {
                if (c[i] != null && c[i].isSimilar(it)) {
                    int take = Math.min(rest, c[i].getAmount());
                    rest -= take;
                    c[i].setAmount(c[i].getAmount() - take);
                    setItem(i, c[i].getAmount() > 0 ? c[i] : null);
                }
            }
            if (rest > 0) {
                ItemStack s = it.clone();
                s.setAmount(rest);
                left.put(k, s);
            }
        }
        return left;
    }

    public void remove(Material m) {
        ItemStack[] c = getContents();
        for (int i = 0; i < c.length; i++) {
            if (c[i] != null && c[i].getType() == m) {
                setItem(i, null);
            }
        }
    }

    public java.util.ListIterator<ItemStack> iterator() {
        return Arrays.asList(getContents()).listIterator();
    }

    public java.util.ListIterator<ItemStack> iterator(int index) {
        return Arrays.asList(getContents()).listIterator(index);
    }

    public int close() {
        int n = viewers.size();
        for (HumanEntity h : new ArrayList<>(viewers)) {
            h.closeInventory();
        }
        return n;
    }
}
