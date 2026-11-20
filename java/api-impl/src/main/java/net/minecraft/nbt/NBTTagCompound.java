package net.minecraft.nbt;

import java.util.LinkedHashMap;
import java.util.Map;

/**
 * Совместимость со старым NMS-именем {@code NBTTagCompound}.
 *
 * <p>В 26.3 класс называется {@link CompoundTag}; DeluxeMenus 1.14.1
 * ({@code NbtProvider}) ищет именно {@code net.minecraft.nbt.NBTTagCompound}
 * и вызывает методы по «обфусцированным» именам из своей таблицы версий
 * (`e`, `l`, `a`, `r`). Без этого класса статический инициализатор
 * провайдера падал, и плагин писал
 * {@code Could not setup a NMS hook for your server version}.
 *
 * <p>Реализация — простое map-хранилище: hook инициализируется и NBT-опции
 * предметов не падают. Значения живут в Java-обёртке предмета; перенос в
 * Rust-стек предметов — отдельная задача.
 */
public class NBTTagCompound {
    private final Map<String, Object> values = new LinkedHashMap<>();

    public NBTTagCompound() {
    }

    // --- «обфусцированные» имена, которые ищет DeluxeMenus ---

    public boolean e(String key) {
        return this.values.containsKey(key);
    }

    public String l(String key) {
        Object value = this.values.get(key);
        return value == null ? "" : String.valueOf(value);
    }

    public void a(String key, String value) {
        this.values.put(key, value);
    }

    public void a(String key, boolean value) {
        this.values.put(key, value);
    }

    public void a(String key, byte value) {
        this.values.put(key, value);
    }

    public void a(String key, short value) {
        this.values.put(key, value);
    }

    public void a(String key, int value) {
        this.values.put(key, value);
    }

    public void r(String key) {
        this.values.remove(key);
    }

    // --- обычные имена: другие плагины и наш код ---

    public boolean hasKey(String key) {
        return this.values.containsKey(key);
    }

    public boolean contains(String key) {
        return this.values.containsKey(key);
    }

    public String getString(String key) {
        return this.l(key);
    }

    public void putString(String key, String value) {
        this.a(key, value);
    }

    public boolean getBoolean(String key) {
        return Boolean.TRUE.equals(this.values.get(key));
    }

    public void putBoolean(String key, boolean value) {
        this.a(key, value);
    }

    public byte getByte(String key) {
        Object value = this.values.get(key);
        return value instanceof Number n ? n.byteValue() : 0;
    }

    public void putByte(String key, byte value) {
        this.a(key, value);
    }

    public short getShort(String key) {
        Object value = this.values.get(key);
        return value instanceof Number n ? n.shortValue() : 0;
    }

    public void putShort(String key, short value) {
        this.a(key, value);
    }

    public int getInt(String key) {
        Object value = this.values.get(key);
        return value instanceof Number n ? n.intValue() : 0;
    }

    public void putInt(String key, int value) {
        this.a(key, value);
    }

    public void remove(String key) {
        this.r(key);
    }

    public java.util.Set<String> getAllKeys() {
        return java.util.Set.copyOf(this.values.keySet());
    }
}
