package org.bukkit.craftbukkit.persistence;

import java.io.IOException;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Set;
import org.bukkit.NamespacedKey;
import org.bukkit.persistence.PersistentDataAdapterContext;
import org.bukkit.persistence.PersistentDataContainer;
import org.bukkit.persistence.PersistentDataType;

/**
 * CraftBukkit {@code CraftPersistentDataContainer}: простая карта
 * «ключ -> значение». Нужен для item-nbt-api и PDC-путей плагинов.
 */
public class CraftPersistentDataContainer implements PersistentDataContainer {
    private final Map<NamespacedKey, Object> values = new LinkedHashMap<>();
    private final PersistentDataAdapterContext adapterContext;

    public CraftPersistentDataContainer() {
        this(null);
    }

    public CraftPersistentDataContainer(PersistentDataAdapterContext adapterContext) {
        this.adapterContext = adapterContext;
    }

    @Override
    public <P, C> void set(NamespacedKey key, PersistentDataType<P, C> type, C value) {
        if (value == null) {
            this.values.remove(key);
        } else {
            this.values.put(key, value);
        }
    }

    @Override
    public void remove(NamespacedKey key) {
        this.values.remove(key);
    }

    @Override
    public <P, C> boolean has(NamespacedKey key, PersistentDataType<P, C> type) {
        return this.values.containsKey(key);
    }

    @Override
    public boolean has(NamespacedKey key) {
        return this.values.containsKey(key);
    }

    @Override
    @SuppressWarnings("unchecked")
    public <P, C> C get(NamespacedKey key, PersistentDataType<P, C> type) {
        return (C) this.values.get(key);
    }

    @Override
    @SuppressWarnings("unchecked")
    public <P, C> C getOrDefault(NamespacedKey key, PersistentDataType<P, C> type, C defaultValue) {
        C value = (C) this.values.get(key);
        return value == null ? defaultValue : value;
    }

    @Override
    public Set<NamespacedKey> getKeys() {
        return Set.copyOf(this.values.keySet());
    }

    @Override
    public boolean isEmpty() {
        return this.values.isEmpty();
    }

    @Override
    @SuppressWarnings({"unchecked", "rawtypes"})
    public void copyTo(PersistentDataContainer other, boolean replace) {
        for (Map.Entry<NamespacedKey, Object> entry : this.values.entrySet()) {
            if (replace || !other.has(entry.getKey())) {
                other.set(entry.getKey(), (PersistentDataType) PersistentDataType.STRING,
                        String.valueOf(entry.getValue()));
            }
        }
    }

    @Override
    public PersistentDataAdapterContext getAdapterContext() {
        return this.adapterContext;
    }

    @Override
    public byte[] serializeToBytes() throws IOException {
        return new byte[0];
    }

    @Override
    public int getSize() {
        return this.values.size();
    }

    /** {@code CraftPersistentDataContainer.toTagCompound()} (item-nbt-api). */
    public net.minecraft.nbt.CompoundTag toTagCompound() {
        return new net.minecraft.nbt.CompoundTag();
    }

    /** {@code CraftPersistentDataContainer.getRaw()}. */
    public Map<NamespacedKey, Object> getRaw() {
        return this.values;
    }

    /** {@code CraftPersistentDataContainer.putAll(CompoundTag)}. */
    public void putAll(net.minecraft.nbt.CompoundTag tag) {
    }

    @Override
    public void readFromBytes(byte[] bytes, boolean clear) throws IOException {
        if (clear) {
            this.values.clear();
        }
    }
}
