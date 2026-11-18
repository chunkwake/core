package net.minecraft.core.component;

import net.minecraft.world.item.component.CustomData;
import net.minecraft.world.item.component.ItemLore;

/** NMS {@code DataComponents} (ImageFrame). */
public final class DataComponents {
    public static final DataComponentType<CustomData> CUSTOM_DATA = new DataComponentType<>();
    public static final DataComponentType<ItemLore> LORE = new DataComponentType<>();

    private DataComponents() {
    }
}
