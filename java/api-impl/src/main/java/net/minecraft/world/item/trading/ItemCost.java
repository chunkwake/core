package net.minecraft.world.item.trading;

import com.mojang.serialization.Codec;
import java.util.Optional;
import net.minecraft.core.Holder;
import net.minecraft.core.component.DataComponentExactPredicate;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;

/** NMS {@code ItemCost}: стаб для CraftEngine ItemCostProxy. */
public class ItemCost {
    public static final Codec<ItemCost> CODEC = null;
    public static final StreamCodec<ItemCost> STREAM_CODEC = null;
    public static final StreamCodec<Optional<ItemCost>> OPTIONAL_STREAM_CODEC = null;
    public static final Class<ItemCost> CLASS = ItemCost.class;

    private final Holder<Item> item;
    private final int count;
    private final DataComponentExactPredicate components;
    private final ItemStack itemStack;

    public ItemCost(Holder<Item> item, int count, DataComponentExactPredicate components) {
        this.item = item;
        this.count = count;
        this.components = components;
        this.itemStack = null;
    }

    public Holder<Item> item() {
        return this.item;
    }

    public int count() {
        return this.count;
    }

    public DataComponentExactPredicate components() {
        return this.components;
    }

    public ItemStack itemStack() {
        return this.itemStack;
    }
}
