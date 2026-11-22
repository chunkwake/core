package net.minecraft.world.level;

import java.util.List;
import java.util.function.Predicate;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.entity.EntityTypeTest;
import net.minecraft.world.phys.AABB;

/** NMS {@code EntityGetter}. */
public interface EntityGetter {
    List<Entity> getEntities(Entity except, AABB box, Predicate<? super Entity> predicate);

    <T extends Entity> List<T> getEntities(EntityTypeTest<Entity, T> test, AABB box,
            Predicate<? super T> predicate);

    List<? extends net.minecraft.world.entity.player.Player> players();

    default List<Entity> getEntities(Entity except, AABB box) {
        return getEntities(except, box, entity -> true);
    }

    default <T extends Entity> List<T> getEntitiesOfClass(Class<T> type, AABB box,
            Predicate<? super T> predicate) {
        return List.of();
    }

    default <T extends Entity> List<T> getEntitiesOfClass(Class<T> type, AABB box) {
        return getEntitiesOfClass(type, box, entity -> true);
    }

    default boolean isUnobstructed(Entity entity, net.minecraft.world.phys.shapes.VoxelShape shape) {
        return true;
    }
}
