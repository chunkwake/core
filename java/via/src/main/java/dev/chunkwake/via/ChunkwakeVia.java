package dev.chunkwake.via;

import com.viaversion.viaversion.ViaManagerImpl;
import com.viaversion.viaversion.api.Via;
import com.viaversion.viaversion.api.command.ViaCommandSender;
import com.viaversion.viaversion.api.connection.UserConnection;
import com.viaversion.viaversion.api.platform.ViaPlatformLoader;
import com.viaversion.viaversion.api.protocol.version.ProtocolVersion;
import com.viaversion.viaversion.commands.ViaCommandHandler;
import com.viaversion.viaversion.connection.UserConnectionImpl;
import com.viaversion.viaversion.platform.ViaCodecHandler;
import com.viaversion.viaversion.protocol.ProtocolPipelineImpl;
import dev.chunkwake.bridge.ViaBridge;
import dev.chunkwake.bridge.ViaHost;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import io.netty.channel.Channel;
import io.netty.channel.embedded.EmbeddedChannel;
import java.io.File;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.locks.ReentrantLock;
import java.util.logging.Level;
import java.util.logging.Logger;
import org.bukkit.command.CommandSender;
import org.bukkit.entity.Player;

/**
 * ViaVersion как библиотека: соединение клиента другой версии — это
 * {@link UserConnectionImpl} на Netty {@link EmbeddedChannel} с одним
 * {@link ViaCodecHandler}, как в ViaProxy.
 *
 * <p>Пакет от клиента идёт в {@code writeInbound}, от сервера — в
 * {@code writeOutbound}; результат забирается из очередей канала. Туда же
 * попадает всё, что Via отправляет сама ({@code sendRawPacket},
 * {@code sendRawPacketToServer}, отложенные отправки в event loop канала),
 * поэтому порядок пакетов в каждом направлении тот же, что в конвейере Paper.
 */
public final class ChunkwakeVia implements ViaHost {
    /**
     * Один замок на всё: {@link EmbeddedChannel} не потокобезопасен, а Via
     * трогает соединения и из своих задач ({@link Platform#runSync}).
     * Перевод одного пакета — микросекунды, клиентов другой версии единицы.
     */
    static final ReentrantLock LOCK = new ReentrantLock();

    private static final Logger LOG = Logger.getLogger("ViaVersion");
    private static final AtomicLong IDS = new AtomicLong();
    /** Ошибок перевода в журнал на соединение; дальше — в DEBUG. */
    private static final int LOGGED_ERRORS = 3;
    /** `CHUNKWAKE_VIA_TRACE=1`: какие пакеты Via погасила (отладка перевода). */
    private static final boolean TRACE = System.getenv("CHUNKWAKE_VIA_TRACE") != null;

    private final Map<Long, Session> sessions = new ConcurrentHashMap<>();
    private final Map<Channel, Session> byChannel = new ConcurrentHashMap<>();
    private final ViaCommandHandler commands = new ViaCommandHandler();

    public ChunkwakeVia(File pluginsDir, String version, boolean backwards, boolean rewind) {
        Platform platform = new Platform(new File(pluginsDir, "ViaVersion"), version, this);
        List<Runnable> addons = new ArrayList<>();
        if (backwards) {
            File dir = new File(pluginsDir, "ViaBackwards");
            addons.add(() -> new Backwards(dir).init(new File(dir, "config.yml")));
        }
        if (rewind) {
            File dir = new File(pluginsDir, "ViaRewind");
            addons.add(() -> new Rewind(dir).init(new File(dir, "config.yml")));
        }
        ViaManagerImpl.initAndLoad(platform, new Injector(), commands, ViaPlatformLoader.NOOP,
                addons.toArray(Runnable[]::new));
    }

    /** Соединение клиента другой версии. */
    private static final class Session {
        final long id;
        final String address;
        final EmbeddedChannel channel;
        final UserConnectionImpl user;
        /** Via закрыла соединение (кик); отдаётся Rust при следующем вызове. */
        volatile String closed;
        int errors;

        Session(long id, String address, EmbeddedChannel channel, UserConnectionImpl user) {
            this.id = id;
            this.address = address;
            this.channel = channel;
            this.user = user;
        }

        /** Всё, что накопилось в очередях канала, — в Rust. */
        boolean drain() {
            channel.runPendingTasks();
            for (Object m; (m = channel.readInbound()) != null; ) {
                emit(SERVERBOUND, m);
            }
            for (Object m; (m = channel.readOutbound()) != null; ) {
                emit(CLIENTBOUND, m);
            }
            if (closed == null && !channel.isOpen()) {
                closed = "Disconnected by ViaVersion";
            }
            if (closed != null) {
                ViaBridge.closed(closed);
                return false;
            }
            return true;
        }

        private static void emit(int direction, Object message) {
            if (message instanceof ByteBuf buf) {
                try {
                    ViaBridge.emit(direction, buf.nioBuffer());
                } finally {
                    buf.release();
                }
            }
        }

        /**
         * Ошибка перевода. Paper в таком случае рвёт соединение
         * («Internal Exception»), но у нас её чаще вызовет расхождение нашего
         * пакета с ванильным, чем сам клиент: пакет пропускается, игрок
         * остаётся, а в журнале видно, какой пакет не перевёлся.
         */
        void failed(int direction, Throwable t) {
            errors++;
            String what = address + " (" + user.getProtocolInfo().protocolVersion() + ", "
                    + (direction == SERVERBOUND ? "serverbound" : "clientbound") + ")";
            if (errors <= LOGGED_ERRORS) {
                LOG.log(Level.WARNING, "packet not translated for " + what + ": " + t
                        + (errors == LOGGED_ERRORS ? " (further errors for this connection go to debug)" : ""));
            }
            ViaBridge.debug("ViaVersion: packet not translated for " + what + ": " + t);
        }
    }

    @Override
    public long open(int protocol, String address) {
        if (!ProtocolVersion.isRegistered(protocol)) {
            return -1;
        }
        LOCK.lock();
        try {
            EmbeddedChannel channel = new EmbeddedChannel();
            UserConnectionImpl user = new UserConnectionImpl(channel, false);
            new ProtocolPipelineImpl(user);
            channel.pipeline().addLast(ViaCodecHandler.NAME, new ViaCodecHandler(user));
            Session s = new Session(IDS.incrementAndGet(), address, channel, user);
            sessions.put(s.id, s);
            byChannel.put(channel, s);
            return s.id;
        } finally {
            LOCK.unlock();
        }
    }

    @Override
    public boolean translate(long id, int direction, byte[] packet) {
        Session s = sessions.get(id);
        if (s == null) {
            ViaBridge.closed("translation session closed");
            return false;
        }
        LOCK.lock();
        try {
            ByteBuf buf = Unpooled.wrappedBuffer(packet);
            try {
                if (direction == SERVERBOUND) {
                    s.channel.writeInbound(buf);
                } else {
                    s.channel.writeOutbound(buf);
                }
                if (TRACE && s.channel.outboundMessages().isEmpty() && s.channel.inboundMessages().isEmpty()) {
                    ViaBridge.debug("ViaVersion: " + (direction == SERVERBOUND ? "serverbound" : "clientbound")
                            + " packet 0x" + Integer.toHexString(packet.length > 0 ? packet[0] & 0xff : -1)
                            + " (" + packet.length + " bytes) produced nothing");
                }
            } catch (Throwable t) {
                s.failed(direction, t);
            }
            return s.drain();
        } finally {
            LOCK.unlock();
        }
    }

    @Override
    public boolean poll(long id) {
        Session s = sessions.get(id);
        if (s == null) {
            return true;
        }
        LOCK.lock();
        try {
            return s.drain();
        } finally {
            LOCK.unlock();
        }
    }

    @Override
    public void close(long id) {
        Session s = sessions.remove(id);
        if (s == null) {
            return;
        }
        LOCK.lock();
        try {
            byChannel.remove(s.channel);
            // Закрытие канала снимает соединение с учёта Via (`onDisconnect`).
            s.channel.close();
            s.channel.runPendingTasks();
            s.channel.finishAndReleaseAll();
        } finally {
            LOCK.unlock();
        }
    }

    /** Кик от Via: соединение закроется при следующем обращении Rust. */
    boolean kick(UserConnection user, String reason) {
        Session s = byChannel.get(user.getChannel());
        if (s == null) {
            return false;
        }
        s.closed = reason == null || reason.isEmpty() ? "Disconnected by ViaVersion" : reason;
        return true;
    }

    @Override
    public int protocol(UUID player) {
        UserConnection user = Via.getManager().getConnectionManager().getConnectedClient(player);
        return user == null ? -1 : user.getProtocolInfo().protocolVersion().getVersion();
    }

    @Override
    public boolean command(CommandSender sender, String[] args) {
        return commands.onCommand(new Sender(sender), args);
    }

    @Override
    public List<String> complete(CommandSender sender, String[] args) {
        return commands.onTabComplete(new Sender(sender), args);
    }

    @Override
    public void disable() {
        ((ViaManagerImpl) Via.getManager()).destroy();
    }

    /** Отправитель команды Bukkit для обработчика команд Via. */
    private record Sender(CommandSender sender) implements ViaCommandSender {
        private static final UUID CONSOLE = new UUID(0, 0);

        @Override
        public boolean hasPermission(String permission) {
            return sender.hasPermission(permission);
        }

        @Override
        public void sendMessage(String message) {
            sender.sendMessage(message);
        }

        @Override
        public UUID getUUID() {
            return sender instanceof Player p ? p.getUniqueId() : CONSOLE;
        }

        @Override
        public String getName() {
            return sender.getName();
        }
    }
}
