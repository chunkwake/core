package net.minecraft.core;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;

/**
 * Рабочий NMS {@code IdMapper}: двусторонняя связь объект ↔ числовой id.
 * Axiom регистрирует в нём block states через {@code add}/{@code addMapping}.
 */
public class IdMapper<T> implements IdMap<T>, Iterable<T> {
    private int nextId;
    private final Map<T, Integer> toId = new HashMap<>();
    private final List<T> byId = new ArrayList<>();

    public IdMapper() {
        this(512);
    }

    public IdMapper(int capacity) {
        ((ArrayList<T>) this.byId).ensureCapacity(Math.max(0, capacity));
    }

    /** Полная перезагрузка реестра (BlockBootstrap заменяет заглушки реальными id). */
    public void clear() {
        this.toId.clear();
        this.byId.clear();
        this.nextId = 0;
    }

    public void addMapping(T value, int id) {
        if (id < 0) {
            throw new IllegalArgumentException("id must be >= 0: " + id);
        }
        this.toId.put(value, id);
        while (this.byId.size() <= id) {
            this.byId.add(null);
        }
        this.byId.set(id, value);
        if (id >= this.nextId) {
            this.nextId = id + 1;
        }
    }

    public void add(T value) {
        this.addMapping(value, this.nextId);
    }

    @Override
    public int getId(T value) {
        return this.toId.getOrDefault(value, -1);
    }

    @Override
    public T byId(int id) {
        return id >= 0 && id < this.byId.size() ? this.byId.get(id) : null;
    }

    public int size() {
        return this.byId.size();
    }

    @Override
    public Iterator<T> iterator() {
        return this.byId.iterator();
    }
}
