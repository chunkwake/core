package net.minecraft.core;

import net.minecraft.resources.Identifier;

/** NMS {@code DefaultedRegistry} — интерфейс (Orebfuscator). */
public interface DefaultedRegistry<T> extends Registry<T> {
    Identifier getDefaultKey();

    T defaultValue();
}
