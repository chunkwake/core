package net.minecraft.world;

/** NMS {@code WorldlyContainer} (CraftEngine). */
public interface WorldlyContainer extends Container {
    int[] getSlotsForFace(net.minecraft.core.Direction side);
}
