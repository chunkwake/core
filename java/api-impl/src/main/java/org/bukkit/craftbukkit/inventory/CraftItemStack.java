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
        return new ItemStack(stack.getType().getKey().toString(), stack.getAmount());
    }

    public static ItemStack asCraftCopy(org.bukkit.inventory.ItemStack stack) {
        return asNMSCopy(stack);
    }

    public static org.bukkit.inventory.ItemStack asCraftMirror(ItemStack stack) {
        return asBukkitCopy(stack);
    }

    public static org.bukkit.inventory.ItemStack asBukkitCopy(ItemStack stack) {
        if (stack == null || stack.isEmpty()) {
            return new org.bukkit.inventory.ItemStack(Material.AIR);
        }
        Material material = Material.matchMaterial(stack.getItemId());
        return new org.bukkit.inventory.ItemStack(
                material == null ? Material.AIR : material, stack.getCount());
    }
}
