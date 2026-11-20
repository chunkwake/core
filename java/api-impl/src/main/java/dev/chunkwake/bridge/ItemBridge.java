package dev.chunkwake.bridge;

/**
 * Публичный мост между Bukkit-стеком и сериализованной моделью
 * {@link StackImpl} (`id count [json]`).
 *
 * <p>Нужен CraftItemStack и NMS {@code ItemStack.CODEC}: они лежат в других
 * пакетах и не видят package-private {@code StackImpl.line/parse}. Через этот
 * мост компоненты предмета (display name, lore, PDC, зачарования)
 * переживают round-trip `Bukkit -> NMS -> NBT -> NMS -> Bukkit`.
 */
public final class ItemBridge {
    private ItemBridge() {
    }

    /** Сериализованный стек или {@code null} для пустого предмета. */
    public static String line(org.bukkit.inventory.ItemStack stack) {
        return StackImpl.line(stack);
    }

    /** Восстанавливает Bukkit-стек из {@link #line}. */
    public static org.bukkit.inventory.ItemStack parse(String line) {
        return StackImpl.parse(line);
    }
}
