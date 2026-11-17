package org.bukkit.craftbukkit;

/**
 * Маркер современного Paper (1.20.5+): версионного
 * пакета больше нет. Плагины проверяют наличие этого
 * класса ({@code Class.forName}) при определении версии NMS
 * (например EssentialsX {@code ReflUtil.getNmsVersionObject}).
 */
public final class CraftServer {
    private CraftServer() {
    }
}
