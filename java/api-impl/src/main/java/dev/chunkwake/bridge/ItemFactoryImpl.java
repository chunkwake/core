package dev.chunkwake.bridge;

import org.bukkit.Material;
import org.bukkit.inventory.ItemFactory;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.ItemMeta;

/**
 * {@link ItemFactory} в объёме CraftItemFactory для меты: пустая мета по типу,
 * сравнение (нет меты — то же, что пустая) и перенос меты на другой тип.
 * Остальное — мягкие заглушки.
 */
final class ItemFactoryImpl {
    static final ItemFactory API = Stub.of(ItemFactory.class, new ItemFactoryImpl());

    private ItemFactoryImpl() {}

    private static StackImpl.Data data(ItemMeta meta) {
        if (meta == null) {
            return new StackImpl.Data();
        }
        if (Stub.impl(meta) instanceof StackImpl.MetaImpl m) {
            return m.data;
        }
        throw new IllegalArgumentException("Unknown meta " + meta.getClass());
    }

    public ItemMeta getItemMeta(Material type) {
        return type == null || type.isAir() ? null : new StackImpl.MetaImpl(new StackImpl.Data(), type).api;
    }

    public boolean equals(ItemMeta a, ItemMeta b) {
        return a == b || data(a).equals(data(b));
    }

    public ItemMeta asMetaFor(ItemMeta meta, Material type) {
        if (type == null || type.isAir()) {
            return null;
        }
        return new StackImpl.MetaImpl(data(meta).copy(), type).api;
    }

    public ItemMeta asMetaFor(ItemMeta meta, ItemStack stack) {
        return asMetaFor(meta, stack == null ? null : stack.getType());
    }

    public boolean isApplicable(ItemMeta meta, Material type) {
        return type != null && !type.isAir();
    }

    public boolean isApplicable(ItemMeta meta, ItemStack stack) {
        return stack != null && isApplicable(meta, stack.getType());
    }
}
