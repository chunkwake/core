package net.minecraft.nbt;

/** Базовый NMS-тег NBT (в 26.x — интерфейс). */
public interface Tag {
    byte getId();

    default String getAsString() {
        return this.toString();
    }
}
