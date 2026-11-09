package dev.chunkwake.via;

import com.viaversion.viaversion.api.connection.UserConnection;
import com.viaversion.viaversion.api.platform.PlatformTask;
import com.viaversion.viaversion.platform.UserConnectionViaVersionPlatform;
import dev.chunkwake.bridge.ViaBridge;
import java.io.File;
import java.util.UUID;
import java.util.logging.Logger;
import net.kyori.adventure.text.serializer.gson.GsonComponentSerializer;
import net.kyori.adventure.text.serializer.legacy.LegacyComponentSerializer;
import org.bukkit.Bukkit;

/**
 * Платформа Via поверх {@link UserConnectionViaVersionPlatform}: конфиг,
 * логгер и API у неё свои, планировщик — встроенный в Via.
 *
 * <p>Задачи «главного потока» ({@code runSync}) идут под замком перевода:
 * они трогают каналы соединений, а те не потокобезопасны.
 */
final class Platform extends UserConnectionViaVersionPlatform {
    private final String version;
    private final ChunkwakeVia via;

    Platform(File dataFolder, String version, ChunkwakeVia via) {
        super(dataFolder);
        this.version = version;
        this.via = via;
    }

    @Override
    public Logger createLogger(String name) {
        return Logger.getLogger(name);
    }

    @Override
    public String getPlatformName() {
        return "Chunkwake";
    }

    @Override
    public String getPlatformVersion() {
        return Bukkit.getServer() != null ? Bukkit.getVersion() : "unknown";
    }

    @Override
    public String getPluginVersion() {
        return version;
    }

    private static Runnable locked(Runnable task) {
        return () -> {
            ChunkwakeVia.LOCK.lock();
            try {
                task.run();
            } finally {
                ChunkwakeVia.LOCK.unlock();
            }
        };
    }

    @Override
    public PlatformTask runSync(Runnable runnable) {
        return super.runSync(locked(runnable));
    }

    @Override
    public PlatformTask runSync(Runnable runnable, long delay) {
        return super.runSync(locked(runnable), delay);
    }

    @Override
    public PlatformTask runRepeatingSync(Runnable runnable, long period) {
        return super.runRepeatingSync(locked(runnable), period);
    }

    @Override
    public boolean kickPlayer(UserConnection connection, String message) {
        return via.kick(connection, message);
    }

    @Override
    public void sendMessage(UserConnection connection, String message) {
        UUID player = connection.getProtocolInfo().getUuid();
        if (player != null) {
            ViaBridge.sendMessage(player, GsonComponentSerializer.gson()
                    .serialize(LegacyComponentSerializer.legacySection().deserialize(message)));
        }
    }

    @Override
    public boolean hasPlugin(String name) {
        return Bukkit.getServer() != null && Bukkit.getPluginManager().getPlugin(name) != null;
    }
}
