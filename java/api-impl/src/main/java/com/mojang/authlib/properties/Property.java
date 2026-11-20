package com.mojang.authlib.properties;

/** Минимальный authlib {@code Property} (textures/value/signature). */
public class Property {
    private final String name;
    private final String value;
    private final String signature;

    public Property(String name, String value) {
        this(name, value, null);
    }

    public Property(String name, String value, String signature) {
        this.name = name;
        this.value = value;
        this.signature = signature;
    }

    public String getName() { return this.name; }
    public String getValue() { return this.value; }
    public String getSignature() { return this.signature; }
}
