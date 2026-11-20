package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import net.kyori.adventure.text.Component;
import net.minecraft.nbt.CompoundTag;
import org.bukkit.Material;
import org.bukkit.inventory.meta.ItemMeta;
import org.junit.jupiter.api.Test;

/**
 * Round-trip компонентов предмета: Bukkit -> NMS -> CompoundTag (CODEC) ->
 * NMS -> Bukkit. Именно это проверяет nightcore в {@code testNbt}: после
 * перекодирования {@code ItemStack.isSimilar} должен остаться true, то есть
 * display name и lore не должны теряться.
 */
class ItemCodecRoundTripTest {
    private static StackImpl sword() {
        StackImpl stack = new StackImpl(Material.DIAMOND_SWORD, 1);
        ItemMeta meta = stack.getItemMeta();
        meta.displayName(Component.text("Test Item"));
        meta.lore(List.of(
                Component.text("Test Lore 1"),
                Component.text("Test Lore 2"),
                Component.text("Test Lore 3")));
        stack.setItemMeta(meta);
        return stack;
    }

    @Test
    void codecPreservesDisplayNameAndLore() {
        StackImpl bukkit = sword();
        net.minecraft.world.item.ItemStack nms =
                org.bukkit.craftbukkit.inventory.CraftItemStack.asNMSCopy(bukkit);
        assertFalse(nms.meta.isEmpty(), "meta must carry components");

        @SuppressWarnings("unchecked")
        com.mojang.serialization.DataResult<Object> encoded =
                (com.mojang.serialization.DataResult<Object>) (Object)
                        net.minecraft.world.item.ItemStack.CODEC.encode(nms, null, null);
        CompoundTag tag = (CompoundTag) encoded.result().orElseThrow();
        assertEquals("minecraft:diamond_sword", tag.getStringOr("id", ""));
        assertEquals(1, tag.getIntOr("count", 0));
        assertTrue(tag.contains("meta"), "components must be encoded");

        @SuppressWarnings("unchecked")
        com.mojang.serialization.DataResult<com.mojang.datafixers.util.Pair<
                        net.minecraft.world.item.ItemStack, Object>> decoded =
                (com.mojang.serialization.DataResult<com.mojang.datafixers.util.Pair<
                                net.minecraft.world.item.ItemStack, Object>>) (Object)
                        net.minecraft.world.item.ItemStack.CODEC.decode(null, tag);
        net.minecraft.world.item.ItemStack restored = decoded.result().orElseThrow().getFirst();
        org.bukkit.inventory.ItemStack back =
                org.bukkit.craftbukkit.inventory.CraftItemStack.asBukkitCopy(restored);

        assertTrue(bukkit.isSimilar(back), "components must survive the round-trip");
        assertEquals(bukkit.getAmount(), back.getAmount());
    }
}
