package org.bukkit.craftbukkit.inventory;

import net.minecraft.world.item.ItemStack;
import org.bukkit.Material;

/**
 * Рабочая конвертация Bukkit ↔ NMS {@code ItemStack} (id + количество).
 * Нужна ProtocolLib для {@code MinecraftGenerator} и item-пакетов.
 */
public final class CraftItemStack extends org.bukkit.inventory.ItemStack {
    private CraftItemStack() {
        super(org.bukkit.Material.AIR);
    }

    public static ItemStack asNMSCopy(org.bukkit.inventory.ItemStack stack) {
        if (stack == null || stack.getType() == Material.AIR) {
            return new ItemStack("minecraft:air", 0);
        }
        ItemStack nms = new ItemStack(stack.getType().getKey().toString(), stack.getAmount());
        // Компоненты (display name, lore, PDC, зачарования) переносим
        // строкой Rust-модели: иначе CODEC сериализует только id+count.
        nms.meta = dev.chunkwake.bridge.ItemBridge.line(stack);
        return nms;
    }

    public static ItemStack asCraftCopy(org.bukkit.inventory.ItemStack stack) {
        return asNMSCopy(stack);
    }

    public static org.bukkit.inventory.ItemStack asCraftMirror(ItemStack stack) {
        return asBukkitCopy(stack);
    }

    /** Paper-метод {@code asBukkitMirror}: копия без клонирования NMS-стека. */
    public static org.bukkit.inventory.ItemStack asBukkitMirror(ItemStack stack) {
        return asBukkitCopy(stack);
    }

    public static org.bukkit.inventory.ItemStack asBukkitCopy(ItemStack stack) {
        if (stack == null || stack.isEmpty()) {
            return new org.bukkit.inventory.ItemStack(Material.AIR);
        }
        if (stack.meta != null && !stack.meta.isEmpty()) {
            org.bukkit.inventory.ItemStack parsed = dev.chunkwake.bridge.ItemBridge.parse(stack.meta);
            if (parsed != null) {
                return parsed;
            }
        }
        Material material = Material.matchMaterial(stack.getItemId());
        return new org.bukkit.inventory.ItemStack(
                material == null ? Material.AIR : material, stack.getCount());
    }
}
