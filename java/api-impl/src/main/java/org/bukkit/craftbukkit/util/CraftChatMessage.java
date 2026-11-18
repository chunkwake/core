package org.bukkit.craftbukkit.util;

import net.minecraft.network.chat.Component;

/**
 * Минимальный CraftBukkit-совместимый {@code CraftChatMessage}.
 *
 * <p>ProtocolLib ищет его при инициализации {@code WrappedChatComponent}
 * ({@code fromString}/{@code fromComponent}).
 */
public final class CraftChatMessage {
    private CraftChatMessage() {
    }

    public static Component[] fromString(String message, boolean keepNewlines) {
        return new Component[] {new Component()};
    }

    public static String fromComponent(Component component) {
        return "";
    }

    public static String fromComponent(Component component, boolean keepNewlines) {
        return "";
    }
}
