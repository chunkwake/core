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

    /**
     * Поля, которые TabTPS ищет рефлексией: тип {@code TickData} и имя,
     * начинающееся с {@code tickTimes}. Нужно минимум три.
     */
    public static final ca.spottedleaf.common.time.TickData tickTimes5s =
            new ca.spottedleaf.common.time.TickData();
    public static final ca.spottedleaf.common.time.TickData tickTimes10s =
            new ca.spottedleaf.common.time.TickData();
    public static final ca.spottedleaf.common.time.TickData tickTimes1m =
            new ca.spottedleaf.common.time.TickData();

    private final ServerTickRateManager tickRateManager = new ServerTickRateManager();
    /** Поле, которое ищет CraftEngine (MinecraftServerProxy#getScoreboard). */
    private final net.minecraft.world.scores.Scoreboard scoreboard =
            new net.minecraft.world.scores.Scoreboard();

    /** Поле, которое ищет CraftEngine (getStructureTemplateManager). */
    private final net.minecraft.world.level.levelgen.structure.StructureTemplateManager structureTemplateManager =
            new net.minecraft.world.level.levelgen.structure.StructureTemplateManager();

    /** {@code MinecraftServer.hasStopped()} (CraftEngine). */
    public boolean hasStopped() {
        return false;
    }

    /** {@code MinecraftServer.registries()} (CraftEngine). */
    public net.minecraft.core.RegistryAccess registries() {
        return net.minecraft.core.RegistryAccess.EMPTY;
    }

    public ServerTickRateManager tickRateManager() {
        return this.tickRateManager;
    }

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

    public void execute(Runnable runnable) {
        runnable.run();
    }

    public net.minecraft.world.level.storage.LevelData.RespawnData getRespawnData() {
        return net.minecraft.world.level.storage.LevelData.RespawnData.DEFAULT;
    }

    public java.nio.file.Path getWorldPath(
            net.minecraft.world.level.storage.LevelResource resource) {
        java.nio.file.Path root = java.nio.file.Path.of(
                System.getProperty("user.dir", ".")).resolve("world");
        if (resource == null || resource.getName().isEmpty()) {
            return root;
        }
        return root.resolve(resource.getName());
    }

    public net.minecraft.server.players.PlayerList getPlayerList() {
        return new net.minecraft.server.players.PlayerList();
    }
}
