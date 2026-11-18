package net.minecraft.server.dedicated;

import net.minecraft.server.MinecraftServer;

/** Минимальный NMS {@code DedicatedServer} (Voxy и другие проверяют класс). */
public class DedicatedServer extends MinecraftServer {
    public static DedicatedServer create() {
        return new DedicatedServer();
    }
}
