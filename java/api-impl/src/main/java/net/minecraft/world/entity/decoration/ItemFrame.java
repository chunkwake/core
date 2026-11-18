package net.minecraft.world.entity.decoration;

import net.minecraft.core.Direction;
import net.minecraft.world.level.Level;

/** NMS {@code ItemFrame}. */
public class ItemFrame extends HangingEntity {
    /** ImageFrame: `ItemFrame.DATA_ITEM`. */
    public static final net.minecraft.network.syncher.EntityDataAccessor<
            net.minecraft.world.item.ItemStack> DATA_ITEM =
            new net.minecraft.network.syncher.EntityDataAccessor<>(8,
                    new net.minecraft.network.syncher.EntityDataSerializer<
                            net.minecraft.world.item.ItemStack>() {
                    });

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
