package net.minecraft.nbt;

import java.util.HashMap;
import java.util.Map;

/** NMS {@code TagType}: тип NBT-тега. */
public final class TagType<T extends Tag> {
    private static final Map<String, TagType<?>> BY_NAME = new HashMap<>();

    private final String name;
    private final int id;

    private TagType(String name, int id) {
        this.name = name;
        this.id = id;
    }

    public static <T extends Tag> TagType<T> create(String name, int id) {
        TagType<T> type = new TagType<>(name, id);
        BY_NAME.put(name, type);
        return type;
    }

    public static TagType<?> byName(String name) {
        return BY_NAME.get(name);
    }

    public String getName() {
        return this.name;
    }

    public int getId() {
        return this.id;
    }

    @Override
    public String toString() {
        return this.name;
    }
}
