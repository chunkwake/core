package net.minecraft.world.entity.item;

import net.minecraft.network.syncher.EntityDataAccessor;
import net.minecraft.network.syncher.EntityDataSerializers;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.item.ItemStack;

/** NMS {@code ItemEntity} (id 8 — после метаданных Entity 0..7). */
public class ItemEntity extends Entity {
    public static final EntityDataAccessor<ItemStack> DATA_ITEM =
            new EntityDataAccessor<>(8, EntityDataSerializers.ITEM_STACK);
}
