package net.minecraft.advancements;

import java.util.Optional;
import net.minecraft.network.chat.Component;
import net.minecraft.world.item.ItemStack;

/** NMS {@code DisplayInfo} (CraftEngine). */
public class DisplayInfo {
    public DisplayInfo() {
    }

    public DisplayInfo(Component title, Component description, ItemStack icon,
            Optional<Object> background, AdvancementType type, boolean showToast,
            boolean announceChat, boolean hidden) {
    }

    public DisplayInfo(Component title, Component description, ItemStack icon,
            Object background, AdvancementType type, boolean showToast,
            boolean announceChat, boolean hidden) {
    }
}
