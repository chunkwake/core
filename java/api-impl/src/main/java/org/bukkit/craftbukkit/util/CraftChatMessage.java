package org.bukkit.craftbukkit.util;

import net.minecraft.network.chat.Component;

/**
 * Минимальный CraftBukkit-совместимый {@code CraftChatMessage}.
 *
 * <p>ProtocolLib ищет его для {@code WrappedChatComponent}.
 */
public final class CraftChatMessage {
    private CraftChatMessage() {
    }

    public static Component[] fromString(String message, boolean keepNewlines) {
        return new Component[] {Component.literal(message == null ? "" : message)};
    }

    public static String fromComponent(Component component) {
        return component == null ? "" : component.getString();
    }

    public static String fromComponent(Component component, boolean keepNewlines) {
        return component == null ? "" : component.getString();
    }
}
