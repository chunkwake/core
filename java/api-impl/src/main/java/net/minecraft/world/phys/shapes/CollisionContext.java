package net.minecraft.world.phys.shapes;

/**
 * NMS {@code CollisionContext}: контекст расчёта столкновений.
 *
 * <p>Нужен только как тип аргумента {@code getShape}/{@code getCollisionShape}.
 */
public interface CollisionContext {
    CollisionContext EMPTY = new CollisionContext() {
    };

    static CollisionContext empty() {
        return EMPTY;
    }
}
