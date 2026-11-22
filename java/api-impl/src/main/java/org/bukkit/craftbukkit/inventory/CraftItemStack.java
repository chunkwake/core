package org.bukkit.craftbukkit.inventory;

import net.minecraft.world.item.ItemStack;
import org.bukkit.Material;

/**
 * CraftBukkit-совместимый {@code CraftItemStack}.
 *
 * <p>CraftEngine проверяет {@code instanceof CraftItemStack} и читает поле
 * {@code handle}. Поэтому это не просто утилита, а реальный наследник
 * {@link org.bukkit.inventory.ItemStack} с NMS-зеркалом.
 */
public class CraftItemStack extends org.bukkit.inventory.ItemStack {
    public ItemStack handle;

    protected CraftItemStack() {
        // no-arg ItemStack(): craftDelegate ещё null; StackImpl перекрывает
        // все методы и не пользуется delegate.
        super();
        this.handle = new ItemStack("minecraft:air", 0);
    }

    protected CraftItemStack(Material material, int amount) {
        // STONE+1 инициализирует craftDelegate, дальше меняем тип/количество.
        super(Material.STONE, 1);
        super.setType(material == null ? Material.AIR : material);
        super.setAmount(Math.max(0, amount));
        this.handle = new ItemStack(
                material == null ? "minecraft:air" : material.getKey().toString(), amount);
    }

    public CraftItemStack(org.bukkit.inventory.ItemStack source) {
        super(Material.STONE, 1);
        super.setType(source == null ? Material.AIR : source.getType());
        super.setAmount(source == null ? 0 : Math.max(0, source.getAmount()));
        if (source != null && source.hasItemMeta()) {
            super.setItemMeta(source.getItemMeta());
        }
        this.handle = asNMSCopy(source);
    }

    public static ItemStack asNMSCopy(org.bukkit.inventory.ItemStack stack) {
        if (stack == null || stack.getType() == Material.AIR) {
            return new ItemStack("minecraft:air", 0);
        }
        ItemStack nms = new ItemStack(stack.getType().getKey().toString(), stack.getAmount());
        nms.meta = dev.chunkwake.bridge.ItemBridge.line(stack);
        return nms;
    }

    public static org.bukkit.inventory.ItemStack asCraftCopy(
            org.bukkit.inventory.ItemStack stack) {
        if (stack instanceof CraftItemStack craft) {
            return craft;
        }
        return new CraftItemStack(stack);
    }

    public static org.bukkit.inventory.ItemStack asCraftMirror(ItemStack stack) {
        if (stack == null || stack.isEmpty()) {
            return new org.bukkit.inventory.ItemStack(Material.AIR);
        }
        org.bukkit.inventory.ItemStack bukkit = asBukkitCopy(stack);
        return new CraftItemStack(bukkit);
    }

    /** Paper-метод {@code asBukkitMirror}: всегда CraftItemStack. */
    public static org.bukkit.inventory.ItemStack asBukkitMirror(ItemStack stack) {
        return asCraftMirror(stack);
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
