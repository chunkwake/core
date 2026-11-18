package net.minecraft.core;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/** Рабочий {@code IdMapper}: двусторонняя связь объект ↔ числовой id. */
public class IdMapper<T> {
    private final Map<T, Integer> toId = new HashMap<>();
    private final List<T> byId = new ArrayList<>();

    public void addMapping(T value, int id) {
        this.toId.put(value, id);
        while (this.byId.size() <= id) {
            this.byId.add(null);
        }
        this.byId.set(id, value);
    }

    public int getId(T value) {
        return this.toId.getOrDefault(value, -1);
    }

    public T byId(int id) {
        return id >= 0 && id < this.byId.size() ? this.byId.get(id) : null;
    }

    public int size() {
        return this.byId.size();
    }

    public java.util.Iterator<T> iterator() {
        return this.byId.iterator();
    }
}
