package net.minecraft.world.entity.decoration;

import net.minecraft.core.Direction;
import net.minecraft.world.level.Level;

/** NMS {@code ItemFrame}. */
public class ItemFrame extends HangingEntity {
    /** ImageFrame: `ItemFrame.DATA_ITEM` (id 9: HangingEntity занимает 8). */
    public static final net.minecraft.network.syncher.EntityDataAccessor<
            net.minecraft.world.item.ItemStack> DATA_ITEM =
            new net.minecraft.network.syncher.EntityDataAccessor<>(9,
                    net.minecraft.network.syncher.EntityDataSerializers.ITEM_STACK);
    public static final net.minecraft.network.syncher.EntityDataAccessor<Integer>
            DATA_ROTATION = new net.minecraft.network.syncher.EntityDataAccessor<>(10,
                    net.minecraft.network.syncher.EntityDataSerializers.INT);

    private int rotation;

    public ItemFrame(Level level, net.minecraft.core.BlockPos pos, Direction direction) {
        super(net.minecraft.world.entity.EntityTypes.ITEM_FRAME, level);
        this.setPosRaw(pos.getX(), pos.getY(), pos.getZ());
    }

    public int getRotation() {
        return this.rotation;
    }

    public void setRotation(int rotation) {
        this.rotation = rotation & 7;
    }
}
