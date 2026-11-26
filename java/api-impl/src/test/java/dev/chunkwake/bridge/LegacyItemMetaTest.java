package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import org.bukkit.Material;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.ItemMeta;
import org.bukkit.inventory.meta.components.CustomModelDataComponent;
import org.junit.jupiter.api.Test;

/** Legacy {@code ItemMeta}: integer {@code CustomModelData} и {@code Unbreakable}. */
class LegacyItemMetaTest {
    private static StackImpl sword() {
        return new StackImpl(Material.DIAMOND_SWORD, 1);
    }

    @Test
    void customModelDataAndUnbreakableSetGetClear() {
        StackImpl stack = sword();
        ItemMeta meta = stack.getItemMeta();
        assertNotNull(meta);
        assertFalse(meta.hasCustomModelData());
        assertFalse(meta.hasCustomModelDataComponent());
        assertFalse(meta.isUnbreakable());
        assertEquals(0, meta.getCustomModelData());

        meta.setCustomModelData(1234);
        meta.setUnbreakable(true);
        assertTrue(meta.hasCustomModelData());
        assertTrue(meta.hasCustomModelDataComponent());
        assertEquals(1234, meta.getCustomModelData());
        assertTrue(meta.isUnbreakable());

        stack.setItemMeta(meta);
        ItemMeta back = stack.getItemMeta();
        assertEquals(1234, back.getCustomModelData());
        assertTrue(back.isUnbreakable());

        ItemMeta copy = back.clone();
        assertEquals(1234, copy.getCustomModelData());
        assertTrue(copy.isUnbreakable());
        copy.setCustomModelData(null);
        copy.setUnbreakable(false);
        assertFalse(copy.hasCustomModelData());
        assertFalse(copy.hasCustomModelDataComponent());
        assertFalse(copy.isUnbreakable());
        assertEquals(0, copy.getCustomModelData());
    }

    @Test
    void customModelDataComponentMirrorsLegacyInteger() {
        StackImpl stack = sword();
        ItemMeta meta = stack.getItemMeta();
        CustomModelDataComponent component = meta.getCustomModelDataComponent();
        assertNotNull(component);
        assertTrue(component.getFloats().isEmpty());

        component.setFloats(List.of(7.0f));
        assertEquals(7, meta.getCustomModelData());
        assertEquals(List.of(7.0f), meta.getCustomModelDataComponent().getFloats());

        meta.setCustomModelData(9);
        assertEquals(List.of(9.0f), meta.getCustomModelDataComponent().getFloats());

        component.setFloats(List.of());
        assertFalse(meta.hasCustomModelData());
        meta.setCustomModelData(3);
        meta.setCustomModelDataComponent(null);
        assertFalse(meta.hasCustomModelData());
    }

    @Test
    void bridgeLineRoundTripPreservesLegacyFields() {
        StackImpl stack = sword();
        ItemMeta meta = stack.getItemMeta();
        meta.setCustomModelData(-42);
        meta.setUnbreakable(true);
        stack.setItemMeta(meta);

        String line = StackImpl.line(stack);
        assertTrue(line.contains("\"custom_model_data\":-42"), line);
        assertTrue(line.contains("\"unbreakable\":true"), line);

        ItemStack parsed = StackImpl.parse(line);
        assertEquals(Material.DIAMOND_SWORD, parsed.getType());
        assertTrue(stack.isSimilar(parsed));
        ItemMeta back = parsed.getItemMeta();
        assertEquals(-42, back.getCustomModelData());
        assertTrue(back.isUnbreakable());

        // Ключи, которые пишет Rust-сервер, читаются и без остальных компонентов.
        ItemStack raw = StackImpl.parse(
                "minecraft:stone 2 {\"custom_model_data\":5,\"unbreakable\":true}");
        ItemMeta rawMeta = raw.getItemMeta();
        assertEquals(5, rawMeta.getCustomModelData());
        assertTrue(rawMeta.isUnbreakable());
        assertTrue(StackImpl.line(raw).contains("\"custom_model_data\":5"), StackImpl.line(raw));

        ItemMeta cleared = parsed.getItemMeta();
        cleared.setCustomModelData(null);
        cleared.setUnbreakable(false);
        parsed.setItemMeta(cleared);
        assertEquals("minecraft:diamond_sword 1", StackImpl.line(parsed));
    }
}
