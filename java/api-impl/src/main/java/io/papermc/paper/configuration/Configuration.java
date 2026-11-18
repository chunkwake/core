package io.papermc.paper.configuration;

/**
 * Маркер современного Paper: PaperLib и другие плагины проверяют наличие этого
 * класса (`PaperLib.isPaper()`), чтобы использовать Paper-API вместо
 * CraftBukkit-рефлексии (например, `Bukkit.getCommandMap()`).
 */
public final class Configuration {
    private Configuration() {
    }
}
