package net.minecraft.server.level;

/** NMS {@code ServerEntity}: FancyNpcs ссылается на вложенный Synchronizer. */
public class ServerEntity {
    /** Поле, которое ищет CraftEngine (setUpdateInterval). */
    private int updateInterval;

    /** Интерфейс синхронизации сущности (FancyNpcs). */
    public interface Synchronizer {
        void sendChanges();

        void sendPairingData(java.util.function.Consumer<Object> consumer);
    }
}
