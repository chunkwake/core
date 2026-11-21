package net.minecraft.core.component;

import java.util.LinkedHashSet;
import java.util.Set;

/** NMS {@code PatchedDataComponentMap}: прототип + патч компонентов. */
public final class PatchedDataComponentMap implements DataComponentMap {
    private final DataComponentMap prototype;
    private final it.unimi.dsi.fastutil.objects.Reference2ObjectMap<DataComponentType<?>, Object>
            patch = new it.unimi.dsi.fastutil.objects.Reference2ObjectOpenHashMap<>();

    public PatchedDataComponentMap(DataComponentMap prototype) {
        this.prototype = prototype;
    }

    /** {@code PatchedDataComponentMap.fromPatch(prototype, patch)} (CraftEngine). */
    public static PatchedDataComponentMap fromPatch(DataComponentMap prototype,
            DataComponentPatch patch) {
        return new PatchedDataComponentMap(prototype);
    }

    @SuppressWarnings("unchecked")
    @Override
    public <T> T get(DataComponentType<? extends T> type) {
        if (this.patch.containsKey(type)) {
            return (T) this.patch.get(type);
        }
        return this.prototype == null ? null : this.prototype.get(type);
    }

    public <T> T set(DataComponentType<T> type, T value) {
        T previous = this.get(type);
        this.patch.put(type, value);
        return previous;
    }

    public <T> T remove(DataComponentType<? extends T> type) {
        T previous = this.get(type);
        this.patch.put(type, null);
        return previous;
    }

    public void clearPatch() {
        this.patch.clear();
    }

    public void applyPatch(DataComponentPatch patch) {
    }

    public void restorePatch(DataComponentPatch patch) {
    }

    public void setAll(DataComponentMap map) {
    }

    @Override
    public Set<DataComponentType<?>> keySet() {
        Set<DataComponentType<?>> keys = new LinkedHashSet<>();
        if (this.prototype != null) {
            keys.addAll(this.prototype.keySet());
        }
        keys.addAll(this.patch.keySet());
        return keys;
    }

    @Override
    public int size() {
        return this.keySet().size();
    }

    @Override
    public DataComponentPatch asPatch() {
        return new DataComponentPatch();
    }

    public PatchedDataComponentMap copy() {
        return new PatchedDataComponentMap(this.prototype);
    }
}
