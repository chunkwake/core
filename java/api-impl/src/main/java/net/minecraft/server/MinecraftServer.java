package net.minecraft.server;

import net.minecraft.server.network.ServerConnection;

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

    public final ServerConnection connection = new ServerConnection();

    public static MinecraftServer getServer() {
        return SERVER;
    }

    public ServerConnection getConnection() {
        return this.connection;
    }
}
