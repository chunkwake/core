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
    /** `PlayerInventory.getViewers`: свой инвентарь видит только владелец. */
    public java.util.List<HumanEntity> getViewers() { return java.util.List.of(player.api); }
    public int getMaxStackSize() { return 99; }

    public ItemStack getItem(int index) {
        ItemStack s = contents()[index];
        return s.isEmpty() ? null : s;
    }

    public void setItem(int index, ItemStack item) {
        if (index < 0 || index >= SIZE) {
            throw new ArrayIndexOutOfBoundsException(index);
        }
        String line = StackImpl.line(item);
        player.control("slot " + index + " " + (line == null ? "-" : line));
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

    /**
     * Ванильный контракт {@code PlayerInventory}: рука/броня никогда не
     * возвращают null — пустой слот отдаёт {@code AIR} (GriefPrevention
     * вызывает {@code getItemInHand().getType()} и падал на null).
     */
    private static ItemStack airIfEmpty(ItemStack stack) {
        return stack == null || stack.isEmpty() ? new StackImpl(Material.AIR, 0) : stack;
    }

    public ItemStack getItemInMainHand() { return airIfEmpty(contents()[getHeldItemSlot()]); }
    public void setItemInMainHand(ItemStack item) { setItem(getHeldItemSlot(), item); }
    /** Устаревший алиас основной руки (плагины CraftBook/UltimateShop и др.). */
    @Deprecated
    public ItemStack getItemInHand() { return getItemInMainHand(); }
    @Deprecated
    public void setItemInHand(ItemStack item) { setItemInMainHand(item); }
    public ItemStack getItemInOffHand() { return airIfEmpty(contents()[OFFHAND]); }
    public ItemStack getItem(org.bukkit.inventory.EquipmentSlot slot) {
        return switch (slot) {
            case HAND -> getItemInMainHand();
            case OFF_HAND -> getItemInOffHand();
            case FEET -> getBoots();
            case LEGS -> getLeggings();
            case CHEST -> getChestplate();
            case HEAD -> getHelmet();
            default -> new StackImpl(Material.AIR, 0);
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

    public ItemStack getHelmet() { return airIfEmpty(contents()[HELMET]); }
    public ItemStack getChestplate() { return airIfEmpty(contents()[CHESTPLATE]); }
    public ItemStack getLeggings() { return airIfEmpty(contents()[LEGGINGS]); }
    public ItemStack getBoots() { return airIfEmpty(contents()[BOOTS]); }
    public void setHelmet(ItemStack s) { setItem(HELMET, s); }
    public void setChestplate(ItemStack s) { setItem(CHESTPLATE, s); }
    public void setLeggings(ItemStack s) { setItem(LEGGINGS, s); }
    public void setBoots(ItemStack s) { setItem(BOOTS, s); }

    /** Порядок Bukkit: ботинки, штаны, нагрудник, шлем. */
    public void setArmorContents(ItemStack[] items) {
        ItemStack[] armor = items == null ? new ItemStack[0] : items;
        setBoots(armor.length > 0 ? armor[0] : null);
        setLeggings(armor.length > 1 ? armor[1] : null);
        setChestplate(armor.length > 2 ? armor[2] : null);
        setHelmet(armor.length > 3 ? armor[3] : null);
    }

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
                    c[i] = c[i].clone();
                    c[i].setAmount(c[i].getAmount() + add);
                    changed.put(i, c[i]);
                    amount -= add;
                }
            }
            for (int i = 0; i < STORAGE && amount > 0; i++) {
                if (c[i].isEmpty()) {
                    int add = Math.min(amount, max);
                    c[i] = item.clone();
                    c[i].setAmount(add);
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
