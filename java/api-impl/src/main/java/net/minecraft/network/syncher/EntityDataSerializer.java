package net.minecraft.network.syncher;

/**
 * NMS {@code EntityDataSerializer}.
 *
 * <p>Числовой id — из реестра 26.3 (`EntityDataSerializers`), он уходит в
 * сетевой пакет set_entity_data.
 */
public interface EntityDataSerializer<T> {
    int id();
}
