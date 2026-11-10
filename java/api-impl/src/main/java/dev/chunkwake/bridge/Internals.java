package dev.chunkwake.bridge;

import java.io.PrintWriter;
import java.io.StringWriter;
import java.util.logging.Level;
import java.util.logging.Logger;

/**
 * Ошибки плагинов, которые лезут во внутренности Java-сервера.
 *
 * Плагины под Paper зовут {@code net.minecraft.*} и CraftBukkit напрямую:
 * {@code MinecraftServer.getServer()}, ванильный {@code Commands} с
 * {@code CommandSourceStack}, {@code RegistryFriendlyByteBuf}. В Chunkwake
 * мир живёт в Rust, ванильного сервера в JVM нет, и такой код не заработает,
 * даже если классы найдутся. Простыня трейса тут ничего не объясняет: вместо
 * неё — одна строка с тем, что именно понадобилось, а трейс уходит в DEBUG.
 */
final class Internals {
    private Internals() {}

    /** Чего не хватило: класс или метод сервера; {@code null} — ошибка не про них. */
    static String missing(Throwable error) {
        for (Throwable t = error; t != null; t = t.getCause()) {
            String message = String.valueOf(t.getMessage());
            if ((t instanceof NoClassDefFoundError || t instanceof ClassNotFoundException
                    || t instanceof NoSuchMethodError || t instanceof NoSuchFieldError
                    || t instanceof NoSuchMethodException || t instanceof NoSuchFieldException)
                    && internal(message.replace('/', '.'))) {
                return message.replace('/', '.');
            }
            // Ванильный класс найден, но реестры не подняты: jar игры без
            // `VanillaBootstrap` или бутстрап упал — статический инициализатор падает.
            if (t instanceof ExceptionInInitializerError || message.startsWith("Not bootstrapped")) {
                String entry = entry(error);
                if (entry != null) {
                    return entry;
                }
            }
        }
        return null;
    }

    /**
     * Ванильный класс, который вызвал сам плагин: последний кадр
     * {@code net.minecraft} перед первым кадром не из сервера и не из JDK.
     */
    private static String entry(Throwable error) {
        for (Throwable t = error; t != null; t = t.getCause()) {
            StackTraceElement[] frames = t.getStackTrace();
            String last = null;
            for (StackTraceElement f : frames) {
                String c = f.getClassName();
                if (internal(c)) {
                    last = c;
                } else if (!c.startsWith("com.mojang.") && !c.startsWith("java.") && !c.startsWith("jdk.")) {
                    if (last != null) {
                        return last;
                    }
                    break;
                }
            }
        }
        return null;
    }

    private static boolean internal(String name) {
        return name.contains("net.minecraft.") || name.contains("org.bukkit.craftbukkit")
                || name.contains("io.papermc.paper.adventure.") || name.contains("ca.spottedleaf.");
    }

    /**
     * Ошибку про внутренности — одной строкой WARN, трейс в DEBUG;
     * {@code false} — ошибка другая, пусть её пишет вызывающий как обычно.
     */
    static boolean report(Logger logger, String plugin, String what, Throwable error) {
        String missing = missing(error);
        if (missing == null) {
            return false;
        }
        logger.log(Level.WARNING, plugin + ": " + what + " needs Minecraft server internals (" + missing
                + "), which Chunkwake does not have; skipped");
        StringWriter trace = new StringWriter();
        error.printStackTrace(new PrintWriter(trace));
        Bridge.log(3, plugin + ": " + what + ":\n" + trace);
        return true;
    }
}
