package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertTrue;

import net.minecraft.nbt.NBTTagCompound;
import net.minecraft.world.item.ItemStack;
import org.junit.jupiter.api.Test;

/** Проверки legacy-совместимости для DeluxeMenus NbtProvider. */
class NbtCompatTest {
    @Test
    void legacyObfuscatedAliasesWork() {
        NBTTagCompound tag = new NBTTagCompound();
        assertFalse(tag.e("key"));
        tag.a("key", "value");
        tag.a("count", 3);
        assertTrue(tag.e("key"));
        assertEquals("value", tag.l("key"));
        assertEquals(3, tag.getInt("count"));
        tag.r("key");
        assertFalse(tag.e("key"));
    }

    @Test
    void itemStackLegacyTagAccessors() {
        ItemStack stack = new ItemStack("minecraft:paper", 1);
        assertFalse(stack.u());
        NBTTagCompound tag = new NBTTagCompound();
        tag.a("display", "x");
        stack.c(tag);
        assertTrue(stack.u());
        assertSame(tag, stack.v());
    }
}
