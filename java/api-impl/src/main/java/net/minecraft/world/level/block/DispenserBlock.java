package net.minecraft.world.level.block;

import java.util.Map;
import net.minecraft.world.level.ItemLike;

/** NMS {@code DispenserBlock}: поведение раздатчика. */
public class DispenserBlock extends Block {
    public static final Map<net.minecraft.world.item.Item, net.minecraft.core.dispenser.DispenseItemBehavior>
            DISPENSER_REGISTRY = new java.util.HashMap<>();

    public DispenserBlock() {
    }

    public DispenserBlock(net.minecraft.world.level.block.state.BlockBehaviour.Properties properties) {
        super(properties);
    }

    public static void registerBehavior(ItemLike item,
            net.minecraft.core.dispenser.DispenseItemBehavior behavior) {
        DISPENSER_REGISTRY.put(item == null ? null : item.asItem(), behavior);
    }

    public static void registerProjectileBehavior(ItemLike item) {
    }
}
