package net.minecraft.server;

import net.minecraft.server.network.ServerConnectionListener;

/**
 * Минимальный NMS-класс для ProtocolLib.
 *
 * <p>ProtocolLib ищет {@code net.minecraft.server.MinecraftServer} при
 * инициализации сетевого инжектора, берёт singleton и вызывает геттер
 * {@code ServerConnection}. Пока у сервера нет настоящего NMS, эти объекты
 * служат точкой опоры для рефлексии.
 */
public class MinecraftServer {
    public static final String VERSION = "26.3";
    public static final MinecraftServer SERVER = new MinecraftServer();

    public final ServerConnectionListener connection = new ServerConnectionListener();

    public static MinecraftServer getServer() {
        return SERVER;
    }

    public net.minecraft.server.network.ServerConnection getConnection() {
        return this.connection;
    }

    /** Тики Chunkwake идут в главном потоке сервера (Axiom, Paper API). */
    public boolean isSameThread() {
        return true;
    }

    public boolean isRunning() {
        return true;
    }

    /** Ключи измерений (Axiom обходит их каждый тик). */
    public java.util.Set<net.minecraft.resources.ResourceKey<net.minecraft.world.level.Level>> levelKeys() {
        return java.util.Set.of();
    }

    public net.minecraft.server.level.ServerLevel getLevel(
            net.minecraft.resources.ResourceKey<net.minecraft.world.level.Level> key) {
        return null;
    }

    /** Все измерения (Axiom обходит их каждый тик). */
    public Iterable<net.minecraft.server.level.ServerLevel> getAllLevels() {
        return java.util.List.of();
    }

    public int getTickCount() {
        return 0;
    }

    /** {@code MinecraftServer.registryAccess()}: реестры для команд/плагинов. */
    public net.minecraft.core.RegistryAccess.Frozen registryAccess() {
        return net.minecraft.core.RegistryAccess.Frozen.empty();
    }

    public net.minecraft.server.players.PlayerList getPlayerList() {
        return new net.minecraft.server.players.PlayerList();
    }
}
