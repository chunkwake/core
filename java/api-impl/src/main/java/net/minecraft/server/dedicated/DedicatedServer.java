package net.minecraft.server.dedicated;

import net.minecraft.server.MinecraftServer;

/** Минимальный NMS {@code DedicatedServer} (Voxy и другие проверяют класс). */
public class DedicatedServer extends MinecraftServer {
    @Override
    public boolean hasStopped() {
        return super.hasStopped();
    }

    @Override
    public net.minecraft.core.RegistryAccess registries() {
        return super.registries();
    }

    /** Поле, которое ищет CraftEngine (DedicatedServerProxy#getSettings). */
    private final DedicatedServerSettings settings = new DedicatedServerSettings();

    private final net.minecraft.commands.Commands commands = new net.minecraft.commands.Commands();

    public static DedicatedServer create() {
        return new DedicatedServer();
    }

    /** `DedicatedServer.getCommands()`: диспетчер Brigadier для плагинов. */
    public net.minecraft.commands.Commands getCommands() {
        return this.commands;
    }
}
