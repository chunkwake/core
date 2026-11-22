package net.minecraft.world.entity.item;

import net.minecraft.network.syncher.EntityDataAccessor;
import net.minecraft.network.syncher.EntityDataSerializers;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.item.ItemStack;

/** NMS {@code ItemEntity} (id 8 — после метаданных Entity 0..7). */
public class ItemEntity extends Entity {
    public ItemEntity(net.minecraft.world.level.Level level, double x, double y, double z,
            ItemStack stack) {
        super(net.minecraft.world.entity.EntityTypes.ITEM, level);
        this.setPos(x, y, z);
    }

    public ItemEntity(net.minecraft.world.level.Level level, double x, double y, double z,
            ItemStack stack, double deltaX, double deltaY, double deltaZ) {
        this(level, x, y, z, stack);
    }

    public static final EntityDataAccessor<ItemStack> DATA_ITEM =
            new EntityDataAccessor<>(8, EntityDataSerializers.ITEM_STACK);
    public int age;
    public int despawnRate = 6000;
    public int pickupDelay;
    public java.util.UUID target;
    private ItemStack stack;

    public ItemStack getItem() {
        return this.stack;
    }

    public void setItem(ItemStack stack) {
        this.stack = stack;
    }

    public void setNoPickUpDelay() {
        this.pickupDelay = 0;
    }

    public void setDefaultPickUpDelay() {
        this.pickupDelay = 10;
    }

    public void makeFakeItem() {
    }

    public void setTarget(java.util.UUID target) {
        this.target = target;
    }

    public void setPickUpDelay(int delay) {
        this.pickupDelay = delay;
    }

    public boolean hasPickUpDelay() {
        return this.pickupDelay > 0;
    }
}
