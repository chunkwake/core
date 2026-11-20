package org.bukkit.craftbukkit.craftbukkit.inventory;

/**
 * Алиас для старых плагинов (DeluxeMenus 1.14.1 {@code NbtProvider}).
 *
 * <p>Такой плагин строит Craft-имя как
 * {@code "org.bukkit.craftbukkit." + NMS_VERSION + "." + class}, где
 * {@code NMS_VERSION} — хвост пакета {@code Bukkit.getServer().getClass()}.
 * У Paper 1.20.5+ (и у нас) пакет без версии, поэтому NMS_VERSION равен
 * {@code "craftbukkit"} и получается
 * {@code org.bukkit.craftbukkit.craftbukkit.inventory.CraftItemStack}.
 * На настоящем Paper этот класс отсутствует и NBT-hook не включается; мы
 * держим делегирующий алиас, чтобы старые плагины работали.
 */
public final class CraftItemStack {
    private CraftItemStack() {
    }

    public static net.minecraft.world.item.ItemStack asNMSCopy(
            org.bukkit.inventory.ItemStack stack) {
        return org.bukkit.craftbukkit.inventory.CraftItemStack.asNMSCopy(stack);
    }

    public static org.bukkit.inventory.ItemStack asBukkitCopy(
            net.minecraft.world.item.ItemStack stack) {
        return org.bukkit.craftbukkit.inventory.CraftItemStack.asBukkitCopy(stack);
    }
}
