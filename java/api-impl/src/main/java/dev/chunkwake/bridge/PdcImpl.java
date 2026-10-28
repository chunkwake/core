package dev.chunkwake.bridge;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import org.bukkit.NamespacedKey;
import org.bukkit.persistence.PersistentDataAdapterContext;
import org.bukkit.persistence.PersistentDataContainer;
import org.bukkit.persistence.PersistentDataType;

/**
 * {@link PersistentDataContainer} поверх NBT-compound ({@code PublicBukkitValues} в
 * {@code custom_data}). Значения — как NBT: Byte, Short, Integer, Long, Float, Double,
 * String, byte[], int[], long[], List и Map (вложенный compound).
 */
final class PdcImpl {
    final PersistentDataContainer api = Stub.of(PersistentDataContainer.class, this);
    final Map<String, Object> tags;

    PdcImpl(Map<String, Object> tags) {
        this.tags = tags;
    }

    private static final PersistentDataAdapterContext CONTEXT =
            Stub.of(PersistentDataAdapterContext.class, new Context());

    static final class Context {
        public PersistentDataContainer newPersistentDataContainer() {
            return new PdcImpl(new LinkedHashMap<>()).api;
        }
    }

    public <P, C> void set(NamespacedKey key, PersistentDataType<P, C> type, C value) {
        java.util.Objects.requireNonNull(value, "The provided value cannot be null");
        tags.put(key.toString(), toNbt(type.toPrimitive(value, CONTEXT)));
    }

    public <P, C> boolean has(NamespacedKey key, PersistentDataType<P, C> type) {
        Object v = tags.get(key.toString());
        return v != null && fromNbt(v, type.getPrimitiveType()) != null;
    }

    public boolean has(NamespacedKey key) {
        return tags.containsKey(key.toString());
    }

    public <P, C> C get(NamespacedKey key, PersistentDataType<P, C> type) {
        Object v = tags.get(key.toString());
        if (v == null) return null;
        Object prim = fromNbt(v, type.getPrimitiveType());
        if (prim == null) {
            throw new IllegalArgumentException("The found tag instance (" + v.getClass().getSimpleName()
                    + ") cannot store " + type.getPrimitiveType().getSimpleName());
        }
        return type.fromPrimitive(type.getPrimitiveType().cast(prim), CONTEXT);
    }

    public <P, C> C getOrDefault(NamespacedKey key, PersistentDataType<P, C> type, C def) {
        C v = has(key, type) ? get(key, type) : null;
        return v == null ? def : v;
    }

    public Set<NamespacedKey> getKeys() {
        Set<NamespacedKey> out = new LinkedHashSet<>();
        for (String k : tags.keySet()) {
            NamespacedKey key = NamespacedKey.fromString(k);
            if (key != null) out.add(key);
        }
        return out;
    }

    public void remove(NamespacedKey key) {
        tags.remove(key.toString());
    }

    public boolean isEmpty() {
        return tags.isEmpty();
    }

    public int getSize() {
        return tags.size();
    }

    public void copyTo(PersistentDataContainer other, boolean replace) {
        if (Stub.impl(other) instanceof PdcImpl o) {
            tags.forEach((k, v) -> {
                if (replace || !o.tags.containsKey(k)) o.tags.put(k, copy(v));
            });
        }
    }

    public PersistentDataAdapterContext getAdapterContext() {
        return CONTEXT;
    }

    public byte[] serializeToBytes() {
        return Nbt.encode(tags).toString().getBytes(java.nio.charset.StandardCharsets.UTF_8);
    }

    public void readFromBytes(byte[] bytes, boolean clear) {
        if (clear) tags.clear();
        String text = new String(bytes, java.nio.charset.StandardCharsets.UTF_8);
        tags.putAll(Nbt.decode(com.google.gson.JsonParser.parseString(text).getAsJsonObject()));
    }

    public void readFromBytes(byte[] bytes) {
        readFromBytes(bytes, true);
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof PdcImpl p && Nbt.encode(p.tags).equals(Nbt.encode(tags));
    }

    @Override
    public int hashCode() {
        return Nbt.encode(tags).hashCode();
    }

    /** Примитив Bukkit → значение NBT. */
    private static Object toNbt(Object prim) {
        if (prim instanceof PersistentDataContainer c && Stub.impl(c) instanceof PdcImpl p) {
            return copy(p.tags);
        }
        if (prim instanceof PersistentDataContainer[] arr) {
            List<Object> out = new ArrayList<>();
            for (PersistentDataContainer c : arr) out.add(toNbt(c));
            return out;
        }
        if (prim instanceof List<?> list) {
            List<Object> out = new ArrayList<>();
            for (Object o : list) out.add(toNbt(o));
            return out;
        }
        if (prim instanceof Boolean b) return (byte) (b ? 1 : 0);
        return prim;
    }

    /** Значение NBT → примитив класса {@code want}; не подходит — {@code null}. */
    private static Object fromNbt(Object v, Class<?> want) {
        if (want == PersistentDataContainer.class) {
            return v instanceof Map<?, ?> m ? new PdcImpl(castMap(m)).api : null;
        }
        if (want == PersistentDataContainer[].class) {
            if (!(v instanceof List<?> l)) return null;
            PersistentDataContainer[] out = new PersistentDataContainer[l.size()];
            for (int i = 0; i < out.length; i++) {
                if (!(l.get(i) instanceof Map<?, ?> m)) return null;
                out[i] = new PdcImpl(castMap(m)).api;
            }
            return out;
        }
        if (want == List.class) {
            if (!(v instanceof List<?> l)) return null;
            List<Object> out = new ArrayList<>();
            for (Object o : l) out.add(o instanceof Map<?, ?> m ? new PdcImpl(castMap(m)).api : o);
            return out;
        }
        return want.isInstance(v) ? v : null;
    }

    @SuppressWarnings("unchecked")
    private static Map<String, Object> castMap(Map<?, ?> m) {
        return (Map<String, Object>) m;
    }

    static Object copy(Object v) {
        if (v instanceof Map<?, ?> m) {
            Map<String, Object> out = new LinkedHashMap<>();
            m.forEach((k, x) -> out.put((String) k, copy(x)));
            return out;
        }
        if (v instanceof List<?> l) {
            List<Object> out = new ArrayList<>();
            for (Object o : l) out.add(copy(o));
            return out;
        }
        if (v instanceof byte[] a) return a.clone();
        if (v instanceof int[] a) return a.clone();
        if (v instanceof long[] a) return a.clone();
        return v;
    }

    /** Типизированный JSON моста ({@code chunkwake_net::stack_nbt}): {@code {"i":5}}, {@code {"c":{…}}}… */
    static final class Nbt {
        static JsonObject encode(Map<String, Object> compound) {
            JsonObject o = new JsonObject();
            compound.forEach((k, v) -> o.add(k, encodeTag(v)));
            return o;
        }

        static JsonObject encodeTag(Object v) {
            JsonObject o = new JsonObject();
            switch (v) {
                case Byte b -> o.addProperty("b", b);
                case Short s -> o.addProperty("s", s);
                case Integer i -> o.addProperty("i", i);
                case Long l -> o.addProperty("l", Long.toString(l));
                case Float f -> o.addProperty("f", f);
                case Double d -> o.addProperty("d", d);
                case String s -> o.addProperty("str", s);
                case byte[] a -> {
                    JsonArray arr = new JsonArray();
                    for (byte x : a) arr.add(x);
                    o.add("ba", arr);
                }
                case int[] a -> {
                    JsonArray arr = new JsonArray();
                    for (int x : a) arr.add(x);
                    o.add("ia", arr);
                }
                case long[] a -> {
                    JsonArray arr = new JsonArray();
                    for (long x : a) arr.add(Long.toString(x));
                    o.add("la", arr);
                }
                case List<?> l -> {
                    JsonArray arr = new JsonArray();
                    for (Object x : l) arr.add(encodeTag(x));
                    o.add("list", arr);
                }
                case Map<?, ?> m -> o.add("c", encode(castMap(m)));
                default -> throw new IllegalArgumentException("Unsupported NBT value " + v.getClass());
            }
            return o;
        }

        static Map<String, Object> decode(JsonObject o) {
            Map<String, Object> out = new LinkedHashMap<>();
            for (Map.Entry<String, JsonElement> e : o.entrySet()) {
                Object v = decodeTag(e.getValue().getAsJsonObject());
                if (v != null) out.put(e.getKey(), v);
            }
            return out;
        }

        static Object decodeTag(JsonObject o) {
            if (o.size() != 1) return null;
            Map.Entry<String, JsonElement> e = o.entrySet().iterator().next();
            JsonElement v = e.getValue();
            return switch (e.getKey()) {
                case "b" -> v.getAsByte();
                case "s" -> v.getAsShort();
                case "i" -> v.getAsInt();
                case "l" -> Long.parseLong(v.getAsString());
                case "f" -> v.getAsFloat();
                case "d" -> v.getAsDouble();
                case "str" -> v.getAsString();
                case "ba" -> {
                    JsonArray a = v.getAsJsonArray();
                    byte[] out = new byte[a.size()];
                    for (int i = 0; i < out.length; i++) out[i] = a.get(i).getAsByte();
                    yield out;
                }
                case "ia" -> {
                    JsonArray a = v.getAsJsonArray();
                    int[] out = new int[a.size()];
                    for (int i = 0; i < out.length; i++) out[i] = a.get(i).getAsInt();
                    yield out;
                }
                case "la" -> {
                    JsonArray a = v.getAsJsonArray();
                    long[] out = new long[a.size()];
                    for (int i = 0; i < out.length; i++) out[i] = Long.parseLong(a.get(i).getAsString());
                    yield out;
                }
                case "list" -> {
                    List<Object> out = new ArrayList<>();
                    for (JsonElement x : v.getAsJsonArray()) {
                        Object t = decodeTag(x.getAsJsonObject());
                        if (t != null) out.add(t);
                    }
                    yield out;
                }
                case "c" -> decode(v.getAsJsonObject());
                default -> null;
            };
        }
    }

    @Override
    public String toString() {
        return Nbt.encode(tags).toString();
    }
}
