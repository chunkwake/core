package dev.chunkwake.bridge;

import java.io.PrintStream;
import java.util.concurrent.CompletableFuture;
import java.util.logging.Level;
import java.util.logging.Logger;

/**
 * Ванильный {@code Bootstrap.bootStrap()} в JVM плагинов.
 *
 * <p>Классы {@code net.minecraft.*} лежат в classpath ({@code game-classes.jar}),
 * но без бутстрапа реестры пусты: {@code BuiltInRegistries}, {@code Items},
 * {@code Commands} падают с «Not bootstrapped». Плагины, которые зовут NMS
 * (commonnetworking, voicechat и т. п.), ждут уже поднятые реестры — как на
 * Paper, где бутстрап делает сам сервер до загрузки плагинов.
 *
 * <p>Бутстрап занимает несколько секунд, поэтому идёт в своём потоке сразу
 * после старта JVM, параллельно с загрузкой мира в Rust; загрузка плагинов
 * ждёт его ({@link #await()}). Ванильный бутстрап подменяет
 * {@code System.out}/{@code err} на log4j — потоки возвращаются обратно.
 */
final class VanillaBootstrap {
    private static final Logger LOG = Logger.getLogger("Chunkwake");
    private static volatile CompletableFuture<Boolean> done = CompletableFuture.completedFuture(false);

    private VanillaBootstrap() {}

    /** Запуск в фоне; без классов игры — ничего не делает. */
    static synchronized void start() {
        if (!done.isDone() || done.getNow(false)) {
            return;
        }
        ClassLoader loader = VanillaBootstrap.class.getClassLoader();
        try {
            Class.forName("net.minecraft.server.Bootstrap", false, loader);
        } catch (ClassNotFoundException | LinkageError e) {
            return;
        }
        CompletableFuture<Boolean> f = new CompletableFuture<>();
        done = f;
        Thread t = new Thread(() -> f.complete(run(loader)), "Vanilla Bootstrap");
        t.setDaemon(true);
        t.start();
    }

    private static boolean run(ClassLoader loader) {
        PrintStream out = System.out, err = System.err;
        long started = System.nanoTime();
        try {
            Class.forName("net.minecraft.SharedConstants", true, loader).getMethod("tryDetectVersion").invoke(null);
            Class.forName("net.minecraft.server.Bootstrap", true, loader).getMethod("bootStrap").invoke(null);
            LOG.info("vanilla registries bootstrapped in " + (System.nanoTime() - started) / 1_000_000 + " ms");
            return true;
        } catch (Throwable t) {
            Throwable cause = t instanceof java.lang.reflect.InvocationTargetException i ? i.getCause() : t;
            LOG.log(Level.WARNING, "vanilla bootstrap failed; plugins using Minecraft internals may not work", cause);
            return false;
        } finally {
            System.setOut(out);
            System.setErr(err);
        }
    }

    /** Дождаться бутстрапа; {@code true} — реестры ванили подняты. */
    static boolean await() {
        return done.join();
    }
}
