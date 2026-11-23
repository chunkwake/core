package org.bukkit.craftbukkit.command;

/** Заглушка CraftBukkit {@code VanillaCommandWrapper} (CommandAPI/GrimAC/PacketEvents). */
public class VanillaCommandWrapper extends org.bukkit.command.Command {
    public VanillaCommandWrapper() {
        super("vanilla");
    }

    @Override
    public boolean execute(org.bukkit.command.CommandSender sender, String commandLabel, String[] args) {
        return false;
    }

    public static Object getListener(org.bukkit.command.CommandSender sender) {
        return null;
    }
}
