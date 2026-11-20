package net.minecraft.world.level;

import java.util.List;
import java.util.function.Predicate;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.entity.EntityTypeTest;
import net.minecraft.world.phys.AABB;

/** Заглушка NMS EntityGetter (CommonLevelAccessor). */
public interface EntityGetter {
    List<Entity> getEntities(Entity except, AABB box, Predicate<? super Entity> predicate);

    <T extends Entity> List<T> getEntities(EntityTypeTest<Entity, T> test, AABB box,
            Predicate<? super T> predicate);

    List<? extends net.minecraft.world.entity.player.Player> players();
}
