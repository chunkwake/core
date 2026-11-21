package org.spigotmc;

/**
 * Spigot-совместимость: некоторые плагины (DeluxeHub) определяют Spigot
 * простым {@code Class.forName("org.spigotmc.SpigotConfig")}. Paper содержит
 * этот класс, поэтому он есть и здесь.
 */
public final class SpigotConfig {
    public static boolean bungeecord;
    /** Алиас, который ищет CraftEngine (SpigotConfigProxy#getBungee). */
    public static boolean bungee = bungeecord;

    private SpigotConfig() {
    }
}
