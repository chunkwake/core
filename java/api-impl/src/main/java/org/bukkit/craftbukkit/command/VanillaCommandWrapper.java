package org.bukkit.craftbukkit.command;

/** Минимальный CraftBukkit {@code VanillaCommandWrapper} (GrimAC/PacketEvents). */
public final class VanillaCommandWrapper {
    private VanillaCommandWrapper() {
    }

    public static Object getListener(org.bukkit.command.CommandSender sender) {
        return null;
    }
}
