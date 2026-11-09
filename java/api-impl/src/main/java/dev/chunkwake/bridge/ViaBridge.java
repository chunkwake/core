package dev.chunkwake.bridge;

import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.UUID;

/** Что мост даёт реализации {@link ViaHost} (она в другом загрузчике классов). */
public final class ViaBridge {
    private ViaBridge() {}

    /** Пакет-результат перевода; зовётся только внутри вызова {@link ViaHost}. */
    public static void emit(int direction, ByteBuffer data) {
        Bridge.viaEmit(direction, data);
    }

    /** Via закрыла соединение: причина уходит в Rust, соединение рвётся. */
    public static void closed(String reason) {
        Bridge.viaEmit(ViaHost.CLOSED, ByteBuffer.wrap(reason.getBytes(StandardCharsets.UTF_8)));
    }

    /** Сообщение игроку в чат (JSON-компонент). */
    public static void sendMessage(UUID player, String json) {
        Bridge.sendMessage(player, json);
    }

    /** Отладочная строка в журнал сервера. */
    public static void debug(String message) {
        Bridge.log(3, message);
    }
}
