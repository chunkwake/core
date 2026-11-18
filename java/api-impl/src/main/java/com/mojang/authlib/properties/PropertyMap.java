package com.mojang.authlib.properties;

import com.google.common.collect.ForwardingMultimap;
import com.google.common.collect.Multimap;

/** Минимальный authlib {@code PropertyMap} поверх Guava Multimap. */
public class PropertyMap extends ForwardingMultimap<String, Property> {
    private final Multimap<String, Property> delegate;

    public PropertyMap(Multimap<String, Property> delegate) {
        this.delegate = delegate;
    }

    @Override
    protected Multimap<String, Property> delegate() {
        return this.delegate;
    }
}
