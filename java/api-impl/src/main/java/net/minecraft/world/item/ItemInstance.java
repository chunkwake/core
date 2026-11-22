package net.minecraft.world.item;

import net.minecraft.core.component.DataComponentGetter;

/** NMS {@code ItemInstance} (26.x): предмет + количество. */
public interface ItemInstance extends net.minecraft.core.TypedInstance<Item>, DataComponentGetter {
    String FIELD_ID = "id";
    String FIELD_COUNT = "count";
    String FIELD_COMPONENTS = "components";

    int count();

    default int getMaxStackSize() {
        return 64;
    }
}
