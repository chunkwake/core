package dev.chunkwake.bridge;

import java.io.File;
import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.nio.charset.StandardCharsets;
import java.util.UUID;
import java.util.logging.Level;
import java.util.logging.Logger;

import static java.lang.foreign.ValueLayout.ADDRESS;
import static java.lang.foreign.ValueLayout.JAVA_INT;
import static java.lang.foreign.ValueLayout.JAVA_LONG;

/**
 * Точка входа из Rust. Rust создаёт JVM и один раз вызывает {@link #boot(long)}
 * с адресом таблицы слотов (u64). Дальше всё общение — через FFM: Java кладёт в
 * таблицу адреса upcall-стабов, Rust — адреса своих функций (downcall).
 *
 * <p>Раскладка таблицы совпадает с {@code crates/bridge/src/table.rs}.
 */
public final class Bridge {
    // Rust → Java (заполняет Java).
    static final int UP_ENABLE = 0;
    static final int UP_TICK = 1;
    static final int UP_JOIN = 2;
    static final int UP_QUIT = 3;
    static final int UP_BLOCK_BREAK = 4;
    static final int UP_COMMAND = 5;
    // Java → Rust (заполняет Rust).
    static final int DOWN_SEND_MESSAGE = 6;
    static final int DOWN_BROADCAST = 7;
    static final int DOWN_LOG = 8;
    // Общая память: Java пишет, Rust читает без вызова.
    static final int FLAG_HAS_TASKS = 9;
    static final int FLAG_LISTENERS = 10;
    // Параметры запуска (заполняет Rust).
    static final int ARG_PLUGINS_DIR = 11;
    static final int ARG_PLUGINS_DIR_LEN = 12;
    static final int DOWN_SET_COMMANDS = 13;
    static final int SLOTS = 14;

    /** Биты маски слушателей. */
    static final long LISTEN_JOIN = 1, LISTEN_QUIT = 2, LISTEN_BLOCK_BREAK = 4;

    static MemorySegment table;
    static MethodHandle sendMessage, broadcast, log, setCommands;
    static ChunkwakeServer server;

    private Bridge() {}

    /** Вызывается из Rust один раз в потоке плагинов. */
    public static void boot(long tableAddress) {
        table = MemorySegment.ofAddress(tableAddress).reinterpret(SLOTS * 8L);
        Linker linker = Linker.nativeLinker();
        sendMessage = down(linker, DOWN_SEND_MESSAGE,
                FunctionDescriptor.ofVoid(JAVA_LONG, JAVA_LONG, ADDRESS, JAVA_INT));
        broadcast = down(linker, DOWN_BROADCAST, FunctionDescriptor.ofVoid(ADDRESS, JAVA_INT));
        log = down(linker, DOWN_LOG, FunctionDescriptor.ofVoid(JAVA_INT, ADDRESS, JAVA_INT));
        setCommands = down(linker, DOWN_SET_COMMANDS, FunctionDescriptor.ofVoid(ADDRESS, JAVA_INT));
        NativeLog.install();

        up(linker, UP_ENABLE, "enable", FunctionDescriptor.of(JAVA_INT));
        up(linker, UP_TICK, "tick", FunctionDescriptor.ofVoid());
        up(linker, UP_JOIN, "join", FunctionDescriptor.ofVoid(JAVA_LONG, JAVA_LONG, ADDRESS, JAVA_INT));
        up(linker, UP_QUIT, "quit", FunctionDescriptor.ofVoid(JAVA_LONG, JAVA_LONG));
        up(linker, UP_BLOCK_BREAK, "blockBreak", FunctionDescriptor.of(JAVA_INT,
                JAVA_LONG, JAVA_LONG, JAVA_INT, JAVA_INT, JAVA_INT, ADDRESS, JAVA_INT));
        up(linker, UP_COMMAND, "command",
                FunctionDescriptor.of(JAVA_INT, JAVA_LONG, JAVA_LONG, ADDRESS, JAVA_INT));
    }

    private static MethodHandle down(Linker linker, int slot, FunctionDescriptor fd) {
        return linker.downcallHandle(MemorySegment.ofAddress(table.getAtIndex(JAVA_LONG, slot)), fd);
    }

    private static void up(Linker linker, int slot, String name, FunctionDescriptor fd) {
        try {
            MethodHandle target = MethodHandles.lookup().findStatic(Bridge.class, name, fd.toMethodType());
            MemorySegment stub = linker.upcallStub(target, fd, Arena.global());
            table.setAtIndex(JAVA_LONG, slot, stub.address());
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    // --- upcalls: исключение из upcall роняет JVM, поэтому всё ловим ---

    static int enable() {
        try {
            String dir = string(MemorySegment.ofAddress(table.getAtIndex(JAVA_LONG, ARG_PLUGINS_DIR)),
                    (int) table.getAtIndex(JAVA_LONG, ARG_PLUGINS_DIR_LEN));
            server = new ChunkwakeServer();
            return server.loadAndEnable(new File(dir));
        } catch (Throwable t) {
            Logger.getLogger("Chunkwake").log(Level.SEVERE, "plugin startup failed", t);
            return -1;
        }
    }

    static void tick() {
        try {
            server.scheduler().tick();
        } catch (Throwable t) {
            Logger.getLogger("Chunkwake").log(Level.SEVERE, "tick failed", t);
        }
    }

    static void join(long hi, long lo, MemorySegment name, int len) {
        try {
            server.join(new UUID(hi, lo), string(name, len));
        } catch (Throwable t) {
            Logger.getLogger("Chunkwake").log(Level.SEVERE, "join failed", t);
        }
    }

    static void quit(long hi, long lo) {
        try {
            server.quit(new UUID(hi, lo));
        } catch (Throwable t) {
            Logger.getLogger("Chunkwake").log(Level.SEVERE, "quit failed", t);
        }
    }

    static int blockBreak(long hi, long lo, int x, int y, int z, MemorySegment key, int len) {
        try {
            return server.blockBreak(new UUID(hi, lo), x, y, z, string(key, len)) ? 1 : 0;
        } catch (Throwable t) {
            Logger.getLogger("Chunkwake").log(Level.SEVERE, "block break failed", t);
            return 0;
        }
    }

    static int command(long hi, long lo, MemorySegment line, int len) {
        try {
            return server.command(new UUID(hi, lo), string(line, len)) ? 1 : 0;
        } catch (Throwable t) {
            Logger.getLogger("Chunkwake").log(Level.SEVERE, "command failed", t);
            return 1;
        }
    }

    // --- downcalls ---

    static void sendMessage(UUID to, String json) {
        try (Arena a = Arena.ofConfined()) {
            byte[] b = json.getBytes(StandardCharsets.UTF_8);
            sendMessage.invokeExact(to.getMostSignificantBits(), to.getLeastSignificantBits(),
                    a.allocateFrom(java.lang.foreign.ValueLayout.JAVA_BYTE, b), b.length);
        } catch (Throwable t) {
            throw new IllegalStateException(t);
        }
    }

    static void broadcast(String json) {
        callText(broadcast, json);
    }

    static void setCommands(String names) {
        callText(setCommands, names);
    }

    static void log(int level, String message) {
        try (Arena a = Arena.ofConfined()) {
            byte[] b = message.getBytes(StandardCharsets.UTF_8);
            log.invokeExact(level, a.allocateFrom(java.lang.foreign.ValueLayout.JAVA_BYTE, b), b.length);
        } catch (Throwable t) {
            t.printStackTrace();
        }
    }

    private static void callText(MethodHandle h, String text) {
        try (Arena a = Arena.ofConfined()) {
            byte[] b = text.getBytes(StandardCharsets.UTF_8);
            h.invokeExact(a.allocateFrom(java.lang.foreign.ValueLayout.JAVA_BYTE, b), b.length);
        } catch (Throwable t) {
            throw new IllegalStateException(t);
        }
    }

    /** Флаги в общей памяти (Rust читает их атомарно). */
    static void setFlag(int slot, long value) {
        table.setAtIndex(JAVA_LONG, slot, value);
    }

    static String string(MemorySegment ptr, int len) {
        return new String(ptr.reinterpret(len).toArray(java.lang.foreign.ValueLayout.JAVA_BYTE),
                StandardCharsets.UTF_8);
    }
}
