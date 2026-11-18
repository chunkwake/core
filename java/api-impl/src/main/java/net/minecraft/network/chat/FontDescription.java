package net.minecraft.network.chat;

import net.minecraft.resources.Identifier;

/** Минимальный NMS {@code FontDescription}. */
public class FontDescription {
    private final Identifier location;

    public FontDescription(Identifier location) {
        this.location = location;
    }

    public Identifier location() {
        return this.location;
    }

    public static final class Resource extends FontDescription {
        public Resource(Identifier location) {
            super(location);
        }
    }
}
