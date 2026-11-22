package net.minecraft.world.phys.shapes;

/**
 * NMS {@code CollisionContext}: контекст расчёта столкновений.
 *
 * <p>Нужен как тип аргумента {@code getShape}/{@code getCollisionShape} и как
 * набор статических фабрик, которые вызывает CraftEngine.
 */
public interface CollisionContext {
    CollisionContext EMPTY = new CollisionContext() {
    };

    static CollisionContext empty() {
        return EMPTY;
    }

    static CollisionContext emptyWithFluidCollisions() {
        return EMPTY;
    }

    static CollisionContext of(net.minecraft.world.entity.Entity entity) {
        return EMPTY;
    }

    static CollisionContext of(net.minecraft.world.entity.Entity entity, boolean alwaysCollide) {
        return EMPTY;
    }

    static CollisionContext placementContext(net.minecraft.world.entity.player.Player player) {
        return EMPTY;
    }

    static CollisionContext positionContext(double position) {
        return EMPTY;
    }

    static CollisionContext withPosition(net.minecraft.world.entity.Entity entity, double position) {
        return EMPTY;
    }

    default boolean isDescending() {
        return false;
    }

    default boolean isPlacement() {
        return false;
    }
}
