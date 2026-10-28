package dev.chunkwake.bridge;

import java.util.HashMap;
import java.util.Map;
import org.bukkit.Material;
import org.bukkit.entity.HumanEntity;
import org.bukkit.event.inventory.InventoryType;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.PlayerInventory;

/**
 * {@link PlayerInventory} игрока: читает зеркало Rust-сессии, изменения шлёт командами.
 * Индексы Bukkit: 0–8 хотбар, 9–35 основной, 36–39 ботинки..шлем, 40 вторая рука.
 */
final class InventoryImpl {
    static final int SIZE = 41, STORAGE = 36, BOOTS = 36, LEGGINGS = 37, CHESTPLATE = 38, HELMET = 39, OFFHAND = 40;

    final PlayerInventory api = Stub.of(PlayerInventory.class, this);
    private final PlayerImpl player;

    InventoryImpl(PlayerImpl player) {
        this.player = player;
    }

    private ItemStack[] contents() {
        return player.state().items();
    }

    public int getSize() { return SIZE; }
    public InventoryType getType() { return InventoryType.PLAYER; }
    public HumanEntity getHolder() { return player.api; }
    public int getMaxStackSize() { return 99; }

    public ItemStack getItem(int index) {
        ItemStack s = contents()[index];
        return s.isEmpty() ? null : s;
    }

    public void setItem(int index, ItemStack item) {
        if (index < 0 || index >= SIZE) {
            throw new ArrayIndexOutOfBoundsException(index);
        }
        String name = StackImpl.name(item);
        player.control(name == null ? "slot " + index + " -" : "slot " + index + " " + name + " " + item.getAmount());
    }

    public ItemStack[] getContents() {
        ItemStack[] c = contents();
        ItemStack[] out = new ItemStack[SIZE];
        for (int i = 0; i < SIZE; i++) {
            out[i] = c[i].isEmpty() ? null : c[i];
        }
        return out;
    }

    public ItemStack[] getStorageContents() {
        return java.util.Arrays.copyOf(getContents(), STORAGE);
    }

    public ItemStack[] getArmorContents() {
        return java.util.Arrays.copyOfRange(getContents(), BOOTS, HELMET + 1);
    }

    public ItemStack[] getExtraContents() {
        return new ItemStack[] {getItem(OFFHAND)};
    }

    public void setContents(ItemStack[] items) {
        for (int i = 0; i < SIZE; i++) {
            setItem(i, i < items.length ? items[i] : null);
        }
    }

    public void setStorageContents(ItemStack[] items) {
        for (int i = 0; i < STORAGE; i++) {
            setItem(i, i < items.length ? items[i] : null);
        }
    }

    public void clear() {
        ItemStack[] c = contents();
        for (int i = 0; i < SIZE; i++) {
            if (!c[i].isEmpty()) setItem(i, null);
        }
    }

    public void clear(int index) { setItem(index, null); }

    public ItemStack getItemInMainHand() { return contents()[getHeldItemSlot()]; }
    public void setItemInMainHand(ItemStack item) { setItem(getHeldItemSlot(), item); }
    public ItemStack getItemInOffHand() { return contents()[OFFHAND]; }
    public ItemStack getItem(org.bukkit.inventory.EquipmentSlot slot) {
        return switch (slot) {
            case HAND -> getItemInMainHand();
            case OFF_HAND -> getItemInOffHand();
            case FEET -> getItem(36);
            case LEGS -> getItem(37);
            case CHEST -> getItem(38);
            case HEAD -> getItem(39);
            default -> new StackImpl(org.bukkit.Material.AIR, 0);
        };
    }
    public void setItemInOffHand(ItemStack item) { setItem(OFFHAND, item); }
    public int getHeldItemSlot() { return player.state().selected(); }

    public void setHeldItemSlot(int slot) {
        if (slot < 0 || slot > 8) {
            throw new IllegalArgumentException("Slot is not between 0 and 8 inclusive");
        }
        player.control("held " + slot);
    }

    public ItemStack getHelmet() { return getItem(HELMET); }
    public ItemStack getChestplate() { return getItem(CHESTPLATE); }
    public ItemStack getLeggings() { return getItem(LEGGINGS); }
    public ItemStack getBoots() { return getItem(BOOTS); }
    public void setHelmet(ItemStack s) { setItem(HELMET, s); }
    public void setChestplate(ItemStack s) { setItem(CHESTPLATE, s); }
    public void setLeggings(ItemStack s) { setItem(LEGGINGS, s); }
    public void setBoots(ItemStack s) { setItem(BOOTS, s); }

    public boolean contains(Material m) {
        for (ItemStack s : contents()) {
            if (!s.isEmpty() && s.getType() == m) return true;
        }
        return false;
    }

    public int first(Material m) {
        ItemStack[] c = contents();
        for (int i = 0; i < STORAGE; i++) {
            if (!c[i].isEmpty() && c[i].getType() == m) return i;
        }
        return -1;
    }

    public int firstEmpty() {
        ItemStack[] c = contents();
        for (int i = 0; i < STORAGE; i++) {
            if (c[i].isEmpty()) return i;
        }
        return -1;
    }

    /** Как {@code CraftInventory.addItem}: сначала неполные стопки, затем пустые слоты. */
    public HashMap<Integer, ItemStack> addItem(ItemStack... items) {
        ItemStack[] c = contents();
        HashMap<Integer, ItemStack> left = new HashMap<>();
        Map<Integer, ItemStack> changed = new java.util.TreeMap<>();
        for (int n = 0; n < items.length; n++) {
            ItemStack item = items[n];
            if (item == null || item.isEmpty()) continue;
            int amount = item.getAmount();
            int max = Math.min(item.getMaxStackSize(), 99);
            for (int i = 0; i < STORAGE && amount > 0; i++) {
                if (!c[i].isEmpty() && c[i].isSimilar(item) && c[i].getAmount() < max) {
                    int add = Math.min(amount, max - c[i].getAmount());
                    c[i] = new StackImpl(c[i].getType(), c[i].getAmount() + add);
                    changed.put(i, c[i]);
                    amount -= add;
                }
            }
            for (int i = 0; i < STORAGE && amount > 0; i++) {
                if (c[i].isEmpty()) {
                    int add = Math.min(amount, max);
                    c[i] = new StackImpl(item.getType(), add);
                    changed.put(i, c[i]);
                    amount -= add;
                }
            }
            if (amount > 0) {
                ItemStack rest = item.clone();
                rest.setAmount(amount);
                left.put(n, rest);
            }
        }
        changed.forEach(this::setItem);
        return left;
    }

    @Override
    public String toString() {
        return "ChunkwakeInventory{" + player.name + "}";
    }
}
