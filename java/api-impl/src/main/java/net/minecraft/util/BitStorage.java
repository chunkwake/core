package net.minecraft.util;

/** NMS {@code BitStorage}: упакованные биты (высоты, палитры). */
public interface BitStorage {
    int get(int index);

    void set(int index, int value);

    long[] getRaw();

    int getSize();
}
