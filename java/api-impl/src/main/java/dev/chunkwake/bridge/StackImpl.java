package dev.chunkwake.bridge;

import java.util.Objects;
import org.bukkit.Material;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.ItemMeta;

/**
 * {@link ItemStack} без компонентов: тип и количество. В Paper это CraftItemStack;
 * здесь — тот же «делегат», что возвращают {@code ItemStack.of} и {@code ItemType.createItemStack}.
 */
final class StackImpl extends ItemStack {
    private Material type;
    private int amount;

    StackImpl(Material type, int amount) {
        this.type = type == null ? Material.AIR : type;
        this.amount = amount;
    }

    /** Стопка по имени предмета Rust-сервера ({@code minecraft:stone}); неизвестное — пусто. */
    static ItemStack of(String name, int count) {
        Material m = Material.matchMaterial(name);
        return new StackImpl(m == null ? Material.AIR : m, m == null ? 0 : count);
    }

    /** Имя для Rust: {@code minecraft:stone}; пустая стопка — {@code null}. */
    static String name(ItemStack s) {
        if (s == null || s.getType() == Material.AIR || s.getAmount() <= 0) {
            return null;
        }
        return s.getType().getKey().toString();
    }

    @Override public Material getType() { return type; }
    @Override public void setType(Material type) { this.type = Objects.requireNonNull(type, "type"); }
    @Override public int getAmount() { return amount; }
    @Override public void setAmount(int amount) { this.amount = amount; }
    @Override public int getMaxStackSize() { return Registries.maxStack(type); }
    @Override public boolean isEmpty() { return type == Material.AIR || amount <= 0; }
    @Override public boolean hasItemMeta() { return false; }
    @Override public ItemMeta getItemMeta() { return null; }
    @Override public boolean setItemMeta(ItemMeta meta) { return meta == null; }

    @Override
    public boolean isSimilar(ItemStack other) {
        return other != null && other.getType() == type;
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof ItemStack s && isSimilar(s) && s.getAmount() == amount;
    }

    @Override
    public int hashCode() {
        return Objects.hash(type, amount);
    }

    @Override
    public StackImpl clone() {
        return new StackImpl(type, amount);
    }

    @Override
    public String toString() {
        return "ItemStack{" + type + " x " + amount + "}";
    }
}
