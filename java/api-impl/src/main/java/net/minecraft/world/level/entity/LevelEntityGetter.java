package net.minecraft.world.level.entity;

import java.util.UUID;
import net.minecraft.world.entity.Entity;

/** NMS {@code LevelEntityGetter}. */
public interface LevelEntityGetter<T extends Entity> {
    T get(int id);

    T get(UUID uuid);

    Iterable<T> getAll();

    default void get(java.util.function.Consumer<T> consumer) {
        this.getAll().forEach(consumer);
    }
}
