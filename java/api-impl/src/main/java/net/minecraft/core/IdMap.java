package net.minecraft.core;

/** Минимальный NMS {@code IdMap} (AxiomPaper). */
public interface IdMap<T> {
    int getId(T value);

    T byId(int id);
}
