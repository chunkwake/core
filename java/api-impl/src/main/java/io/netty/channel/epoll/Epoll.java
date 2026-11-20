package io.netty.channel.epoll;

/**
 * Заглушка netty epoll: на Windows (и в наших тестах) нативный транспорт
 * недоступен, поэтому Plasmo Voice должен выбрать NIO-реализацию.
 */
public final class Epoll {
    private Epoll() {
    }

    public static boolean isAvailable() {
        return false;
    }

    public static boolean isEpollAvailable() {
        return false;
    }
}
