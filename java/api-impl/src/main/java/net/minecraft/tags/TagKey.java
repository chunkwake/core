package net.minecraft.tags;

import net.minecraft.resources.Identifier;

/** NMS {@code TagKey}: имя тега + ключ реестра. */
public final class TagKey<T> {
    private final Object registry;
    private final Identifier location;

    private TagKey(Object registry, Identifier location) {
        this.registry = registry;
        this.location = location;
    }

    public static <T> TagKey<T> create(Object registry, Identifier location) {
        return new TagKey<>(registry, location);
    }

    public Identifier location() {
        return this.location;
    }

    /** 26.x имя. */
    public Identifier identifier() {
        return this.location;
    }

    public Object registry() {
        return this.registry;
    }

    @Override
    public String toString() {
        return "TagKey[" + this.location + "]";
    }
}
