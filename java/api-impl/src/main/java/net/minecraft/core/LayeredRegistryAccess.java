package net.minecraft.core;

import java.util.List;

/** NMS {@code LayeredRegistryAccess} (CraftEngine). */
public class LayeredRegistryAccess<T> implements RegistryAccess.Frozen {
    private final List<T> keys;

    public LayeredRegistryAccess(List<T> keys) {
        this.keys = keys;
    }

    public RegistryAccess.Frozen compositeAccess() {
        return this;
    }

    public RegistryAccess.Frozen getLayer(T key) {
        return this;
    }

    public RegistryAccess.Frozen getAccessForLoading(T key) {
        return this;
    }

    public RegistryAccess.Frozen getAccessFrom(T key) {
        return this;
    }

    public LayeredRegistryAccess<T> replaceFrom(T key, RegistryAccess.Frozen... values) {
        return this;
    }

    public LayeredRegistryAccess<T> replaceFrom(T key, List<RegistryAccess.Frozen> values) {
        return this;
    }
}
