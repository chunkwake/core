package net.minecraft.network.chat;

import net.kyori.adventure.text.Component;

/** NMS {@code CommonComponents} (LevelledMobs ищет класс по имени). */
public final class CommonComponents {
    public static final Component EMPTY = Component.empty();
    public static final Component NEW_LINE = Component.newline();
    public static final Component SPACE = Component.space();
    public static final Component OPTIONAL_SPACE = Component.space();

    private CommonComponents() {
    }
}
