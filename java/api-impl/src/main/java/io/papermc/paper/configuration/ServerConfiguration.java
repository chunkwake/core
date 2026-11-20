package io.papermc.paper.configuration;

/**
 * Paper {@code ServerConfiguration}: конфигурация сервера
 * ({@code Bukkit.getServerConfig()}).
 *
 * <p>Emotecraft проверяет {@code isProxyEnabled()} перед предупреждением о
 * прокси. У нас прокси-режим выключен по умолчанию.
 */
public interface ServerConfiguration {
    default boolean isProxyEnabled() {
        return false;
    }

    /** Пустая реализация: значения Paper по умолчанию. */
    ServerConfiguration EMPTY = new ServerConfiguration() {
    };
}
