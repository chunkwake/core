package net.minecraft.resources;

import com.mojang.serialization.Codec;
import java.util.Objects;

/** Минимальный NMS {@code Identifier} (namespace:path). */
public final class Identifier {
    /** {@code Identifier.CODEC} (CraftEngine loot codec). */
    public static final Codec<Identifier> CODEC =
            Codec.STRING.xmap(Identifier::parse, Identifier::toString);

    private final String namespace;
    private final String path;

    private Identifier(String namespace, String path) {
        this.namespace = namespace;
        this.path = path;
    }

    public static Identifier parse(String value) {
        String input = value == null ? "" : value;
        int colon = input.indexOf(':');
        String namespace = colon < 0 ? "minecraft" : input.substring(0, colon);
        String path = colon < 0 ? input : input.substring(colon + 1);
        return new Identifier(namespace.isEmpty() ? "minecraft" : namespace, path);
    }

    public static Identifier fromNamespaceAndPath(String namespace, String path) {
        return new Identifier(namespace == null || namespace.isEmpty() ? "minecraft" : namespace,
                path == null ? "" : path);
    }

    public String getNamespace() { return this.namespace; }
    public String getPath() { return this.path; }

    @Override public String toString() { return this.namespace + ":" + this.path; }
    @Override public boolean equals(Object other) {
        return other instanceof Identifier i && i.namespace.equals(this.namespace)
                && i.path.equals(this.path);
    }
    @Override public int hashCode() { return Objects.hash(this.namespace, this.path); }

    /** NMS {@code Identifier.tryParse}: null при невалидном id. */
    public static Identifier tryParse(String value) {
        try {
            return parse(value);
        } catch (RuntimeException broken) {
            return null;
        }
    }
}
