package net.minecraft.world.level.levelgen.placement;

/** NMS {@code PlacementContext}: контекст placement (CraftEngine). */
public interface PlacementContext {
    default net.minecraft.world.level.Level getLevel() {
        return null;
    }
}
