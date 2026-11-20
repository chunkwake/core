package net.minecraft.commands;

/** NMS {@code CommandSourceStack}: отправитель команды (Emotecraft). */
public class CommandSourceStack {
    private final net.minecraft.server.level.ServerPlayer player;

    public CommandSourceStack() {
        this(null);
    }

    public CommandSourceStack(net.minecraft.server.level.ServerPlayer player) {
        this.player = player;
    }

    public net.minecraft.server.level.ServerPlayer getPlayerOrException() {
        return this.player;
    }

    public net.minecraft.server.permissions.PermissionSet permissions() {
        return net.minecraft.server.permissions.PermissionSet.all();
    }
}
