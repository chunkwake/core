package dev.chunkwake.bridge;

import java.util.List;
import java.util.UUID;
import org.bukkit.command.CommandSender;

/**
 * ViaVersion, поднятая мостом как библиотека (реализация — в
 * {@code chunkwake-via.jar}, классом {@code dev.chunkwake.via.ChunkwakeVia}).
 *
 * <p>Интерфейс живёт здесь, без классов Via: мост грузит Via и реализацию
 * отдельным загрузчиком и общается с ней только через этот интерфейс.
 * Результат перевода реализация отдаёт через {@link ViaBridge#emit}.
 */
public interface ViaHost {
    /** Направления пакетов (совпадают с {@code crates/bridge/src/via.rs}). */
    int SERVERBOUND = 0, CLIENTBOUND = 1, CLOSED = 2;

    /** Сессия перевода для клиента версии {@code protocol}; {@code -1} — версию не перевести. */
    long open(int protocol, String address);

    /** Переводит пакет; {@code false} — соединение закрыто (причина уже отдана). */
    boolean translate(long id, int direction, byte[] packet);

    /** Пакеты, которые Via отправила сама между вызовами. */
    boolean poll(long id);

    void close(long id);

    /** Версия протокола игрока, если его соединение переводится; иначе {@code -1}. */
    int protocol(UUID player);

    /** {@code /viaversion}. */
    boolean command(CommandSender sender, String[] args);

    List<String> complete(CommandSender sender, String[] args);

    void disable();
}
