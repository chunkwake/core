package net.minecraft.world.level.chunk;

/** NMS {@code PalettedContainerRO}: read-only контейнер палитры. */
public interface PalettedContainerRO<T> {
    T get(int x, int y, int z);

    /** Копия контейнера (squaremap делает снапшот секции). */
    PalettedContainer<T> copy();
}
