package net.minecraft.world.item.component;

import java.util.List;
import net.minecraft.network.chat.Component;

/** NMS {@code ItemLore}. */
public class ItemLore {
    public static final ItemLore EMPTY = new ItemLore(List.of());

    private final List<Component> lines;
    /** Поле, которое ищет CraftEngine (getStyleLines). */
    private final List<Component> styleLines = List.of();

    /** {@code ItemLore.getStyleLines()} (CraftEngine). */
    public List<Component> getStyleLines() {
        return this.lines;
    }

    public ItemLore(List<Component> lines) {
        this.lines = lines == null ? List.of() : List.copyOf(lines);
    }

    public List<Component> lines() {
        return this.lines;
    }
}
