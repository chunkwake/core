package net.minecraft.server;

import com.mojang.brigadier.CommandDispatcher;

/** NMS {@code ServerFunctionLibrary} (CommandAPI). */
public class ServerFunctionLibrary {
    /** {@code ServerFunctionLibrary.dispatcher} (CommandAPI). */
    public final CommandDispatcher<net.minecraft.commands.CommandSourceStack> dispatcher;

    public ServerFunctionLibrary(
            net.minecraft.server.permissions.PermissionSet permissions,
            CommandDispatcher<net.minecraft.commands.CommandSourceStack> dispatcher) {
        this.dispatcher = dispatcher;
    }

    public java.util.Map<net.minecraft.resources.Identifier, Object> getFunctions() {
        return java.util.Map.of();
    }

    public java.util.Optional<Object> getFunction(net.minecraft.resources.Identifier id) {
        return java.util.Optional.empty();
    }

    public java.util.List<Object> getTag(net.minecraft.resources.Identifier id) {
        return java.util.List.of();
    }

    public Iterable<net.minecraft.resources.Identifier> getAvailableTags() {
        return java.util.List.of();
    }
}
