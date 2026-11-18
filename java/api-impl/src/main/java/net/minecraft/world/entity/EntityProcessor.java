package net.minecraft.world.entity;

/** NMS {@code EntityProcessor} (заглушка-интерфейс для Axiom). */
public interface EntityProcessor {
    default void process(Entity entity) {
    }
}
