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
    static final int DOWN_OP_LEVEL = 14;
    static final int DOWN_SET_OP = 15;
    static final int UP_EVENT = 16;
    static final int DOWN_PLAYER_STATE = 17;
    static final int DOWN_PLAYER_CONTROL = 18;
    static final int ARG_ITEMS = 19;
    static final int ARG_ITEMS_LEN = 20;
    static final int DOWN_WORLD = 21;
    static final int SLOTS = 22;

    /** События с ответом ({@code chunkwake_net::live::Event}). */
    static final int EVENT_DAMAGE = 1, EVENT_DEATH = 2, EVENT_DROP = 3, EVENT_PICKUP = 4, EVENT_GAME_MODE = 5,
            EVENT_CHAT = 6, EVENT_COMMAND_PRE = 7, EVENT_TAB_COMPLETE = 8, EVENT_PLACE = 9, EVENT_MOVE = 10,
            EVENT_INTERACT = 11;

    /** Биты маски слушателей. */
    static final long LISTEN_JOIN = 1, LISTEN_QUIT = 2, LISTEN_BLOCK_BREAK = 4;
    /** Бит события {@code kind}: {@code 4 << kind}. */
    static long listenBit(int kind) {
        return 4L << kind;
    }

    static MemorySegment table;
    static MethodHandle sendMessage, broadcast, log, setCommands, opLevel, setOp, playerState, playerControl, world;
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
        opLevel = down(linker, DOWN_OP_LEVEL, FunctionDescriptor.of(JAVA_INT, JAVA_LONG, JAVA_LONG));
        setOp = down(linker, DOWN_SET_OP, FunctionDescriptor.ofVoid(JAVA_LONG, JAVA_LONG, JAVA_INT));
        playerState = down(linker, DOWN_PLAYER_STATE,
                FunctionDescriptor.of(JAVA_INT, JAVA_LONG, JAVA_LONG, ADDRESS, JAVA_INT));
        playerControl = down(linker, DOWN_PLAYER_CONTROL,
                FunctionDescriptor.of(JAVA_INT, JAVA_LONG, JAVA_LONG, ADDRESS, JAVA_INT));
        world = down(linker, DOWN_WORLD, FunctionDescriptor.of(JAVA_INT, ADDRESS, JAVA_INT, ADDRESS, JAVA_INT));
        NativeLog.install();

        up(linker, UP_ENABLE, "enable", FunctionDescriptor.of(JAVA_INT));
        up(linker, UP_TICK, "tick", FunctionDescriptor.ofVoid());
        up(linker, UP_JOIN, "join", FunctionDescriptor.ofVoid(JAVA_LONG, JAVA_LONG, ADDRESS, JAVA_INT));
        up(linker, UP_QUIT, "quit", FunctionDescriptor.ofVoid(JAVA_LONG, JAVA_LONG));
        up(linker, UP_BLOCK_BREAK, "blockBreak", FunctionDescriptor.of(JAVA_INT,
                JAVA_LONG, JAVA_LONG, JAVA_INT, JAVA_INT, JAVA_INT, ADDRESS, JAVA_INT));
        up(linker, UP_COMMAND, "command",
                FunctionDescriptor.of(JAVA_INT, JAVA_LONG, JAVA_LONG, ADDRESS, JAVA_INT));
        up(linker, UP_EVENT, "event", FunctionDescriptor.of(JAVA_INT,
                JAVA_INT, JAVA_LONG, JAVA_LONG, ADDRESS, JAVA_INT, ADDRESS, JAVA_INT));
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
            Registries.loadItems(string(MemorySegment.ofAddress(table.getAtIndex(JAVA_LONG, ARG_ITEMS)),
                    (int) table.getAtIndex(JAVA_LONG, ARG_ITEMS_LEN)));
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

    /** Событие с ответом: текст ответа в {@code out}; длина или -1 (без ответа). */
    static int event(int kind, long hi, long lo, MemorySegment arg, int len, MemorySegment out, int cap) {
        try {
            String reply = server.event(kind, new UUID(hi, lo), string(arg, len));
            byte[] b = reply.getBytes(StandardCharsets.UTF_8);
            if (b.length > cap) {
                return -1;
            }
            out.reinterpret(cap).copyFrom(MemorySegment.ofArray(b));
            return b.length;
        } catch (Throwable t) {
            Logger.getLogger("Chunkwake").log(Level.SEVERE, "event " + kind + " failed", t);
            return -1;
        }
    }

    // --- downcalls ---

    /** Снимок игрока (формат {@code Live::render}); {@code null} — не онлайн. */
    static String playerState(UUID id) {
        int cap = 4096;
        try (Arena a = Arena.ofConfined()) {
            while (true) {
                MemorySegment buf = a.allocate(cap);
                int n = (int) playerState.invokeExact(id.getMostSignificantBits(), id.getLeastSignificantBits(),
                        buf, cap);
                if (n < 0) {
                    return null;
                }
                if (n <= cap) {
                    return string(buf, n);
                }
                cap = n;
            }
        } catch (Throwable t) {
            throw new IllegalStateException(t);
        }
    }

    /** Запрос к миру: {@code get x y z} → имя блока, {@code set x y z имя} → {@code 1}/{@code 0}. */
    static String world(String line) {
        try (Arena a = Arena.ofConfined()) {
            byte[] b = line.getBytes(StandardCharsets.UTF_8);
            MemorySegment out = a.allocate(256);
            int n = (int) world.invokeExact(a.allocateFrom(java.lang.foreign.ValueLayout.JAVA_BYTE, b), b.length,
                    out, 256);
            return string(out, n);
        } catch (Throwable t) {
            throw new IllegalStateException(t);
        }
    }

    static boolean playerControl(UUID id, String line) {
        try (Arena a = Arena.ofConfined()) {
            byte[] b = line.getBytes(StandardCharsets.UTF_8);
            return (int) playerControl.invokeExact(id.getMostSignificantBits(), id.getLeastSignificantBits(),
                    a.allocateFrom(java.lang.foreign.ValueLayout.JAVA_BYTE, b), b.length) != 0;
        } catch (Throwable t) {
            throw new IllegalStateException(t);
        }
    }

    /** Уровень оператора из {@code ops.json} (0 — не оператор). */
    static int opLevel(UUID id) {
        try {
            return (int) opLevel.invokeExact(id.getMostSignificantBits(), id.getLeastSignificantBits());
        } catch (Throwable t) {
            throw new IllegalStateException(t);
        }
    }

    static void setOp(UUID id, boolean op) {
        try {
            setOp.invokeExact(id.getMostSignificantBits(), id.getLeastSignificantBits(), op ? 1 : 0);
        } catch (Throwable t) {
            throw new IllegalStateException(t);
        }
    }

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
