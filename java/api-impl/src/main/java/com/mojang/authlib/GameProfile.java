package com.mojang.authlib;

import com.mojang.authlib.properties.PropertyMap;
import java.util.UUID;

/** Минимальный authlib {@code GameProfile} для NMS-компонентов. */
public class GameProfile {
    private final UUID id;
    private final String name;
    private final PropertyMap properties;

    public GameProfile(UUID id, String name) {
        this(id, name, new PropertyMap(com.google.common.collect.ArrayListMultimap.create()));
    }

    public GameProfile(UUID id, String name, PropertyMap properties) {
        this.id = id;
        this.name = name;
        this.properties = properties;
    }

    public UUID getId() { return this.id; }
    public String getName() { return this.name; }
    public PropertyMap getProperties() { return this.properties; }
}
