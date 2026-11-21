package net.minecraft.advancements;

import net.minecraft.resources.Identifier;

/** NMS {@code AdvancementHolder} (CraftEngine). */
public class AdvancementHolder {
    private final Identifier id;
    private final Advancement value;

    public AdvancementHolder() {
        this(null, null);
    }

    public AdvancementHolder(Identifier id, Advancement value) {
        this.id = id;
        this.value = value;
    }
}
