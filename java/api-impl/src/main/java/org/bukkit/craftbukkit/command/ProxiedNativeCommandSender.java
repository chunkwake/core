package org.bukkit.craftbukkit.command;

import java.util.Set;
import java.util.UUID;
import net.kyori.adventure.text.Component;
import org.bukkit.Server;
import org.bukkit.command.CommandSender;
import org.bukkit.permissions.Permission;
import org.bukkit.permissions.PermissionAttachment;
import org.bukkit.permissions.PermissionAttachmentInfo;
import org.bukkit.plugin.Plugin;

/**
 * CraftBukkit {@code ProxiedNativeCommandSender}: обёртка над NMS-источником
 * команды и Bukkit-отправителем (CommandAPI создаёт её при исполнении).
 */
public class ProxiedNativeCommandSender implements CommandSender {
    private final net.minecraft.commands.CommandSourceStack source;
    private final CommandSender caller;
    private final CommandSender callee;

    public ProxiedNativeCommandSender(net.minecraft.commands.CommandSourceStack source,
            CommandSender caller, CommandSender callee) {
        this.source = source;
        this.caller = caller;
        this.callee = callee;
    }

    public net.minecraft.commands.CommandSourceStack getHandle() {
        return this.source;
    }

    public CommandSender getCaller() {
        return this.caller;
    }

    public CommandSender getCallee() {
        return this.callee;
    }

    @Override
    public void sendMessage(String message) {
        this.callee.sendMessage(message);
    }

    @Override
    public void sendMessage(String... messages) {
        this.callee.sendMessage(messages);
    }

    @Override
    public void sendMessage(UUID sender, String message) {
        this.callee.sendMessage(sender, message);
    }

    @Override
    public void sendMessage(UUID sender, String... messages) {
        this.callee.sendMessage(sender, messages);
    }

    @Override
    public Server getServer() {
        return this.callee.getServer();
    }

    @Override
    public String getName() {
        return this.callee.getName();
    }

    @Override
    public Spigot spigot() {
        return this.callee.spigot();
    }

    @Override
    public Component name() {
        return this.callee.name();
    }

    @Override
    public boolean isPermissionSet(String name) {
        return this.callee.isPermissionSet(name);
    }

    @Override
    public boolean isPermissionSet(Permission perm) {
        return this.callee.isPermissionSet(perm);
    }

    @Override
    public boolean hasPermission(String name) {
        return this.callee.hasPermission(name);
    }

    @Override
    public boolean hasPermission(Permission perm) {
        return this.callee.hasPermission(perm);
    }

    @Override
    public PermissionAttachment addAttachment(Plugin plugin, String name, boolean value) {
        return this.callee.addAttachment(plugin, name, value);
    }

    @Override
    public PermissionAttachment addAttachment(Plugin plugin) {
        return this.callee.addAttachment(plugin);
    }

    @Override
    public PermissionAttachment addAttachment(Plugin plugin, String name, boolean value, int ticks) {
        return this.callee.addAttachment(plugin, name, value, ticks);
    }

    @Override
    public PermissionAttachment addAttachment(Plugin plugin, int ticks) {
        return this.callee.addAttachment(plugin, ticks);
    }

    @Override
    public void removeAttachment(PermissionAttachment attachment) {
        this.callee.removeAttachment(attachment);
    }

    @Override
    public void recalculatePermissions() {
        this.callee.recalculatePermissions();
    }

    @Override
    public Set<PermissionAttachmentInfo> getEffectivePermissions() {
        return this.callee.getEffectivePermissions();
    }

    @Override
    public boolean isOp() {
        return this.callee.isOp();
    }

    @Override
    public void setOp(boolean value) {
        this.callee.setOp(value);
    }
}
