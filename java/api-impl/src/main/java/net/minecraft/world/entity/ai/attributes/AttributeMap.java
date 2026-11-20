package net.minecraft.world.entity.ai.attributes;

import net.minecraft.core.Holder;

/** NMS {@code AttributeMap} (MythicMobs: кастомные атрибуты). */
public class AttributeMap {
    public AttributeMap() {
    }

    public AttributeInstance getInstance(Attribute attribute) {
        return null;
    }

    public AttributeInstance getInstance(Holder<Attribute> attribute) {
        return null;
    }

    public boolean hasAttribute(Attribute attribute) {
        return false;
    }

    public boolean hasAttribute(Holder<Attribute> attribute) {
        return false;
    }

    public double getValue(Attribute attribute) {
        return attribute == null ? 0.0D : attribute.getDefaultValue();
    }

    public double getValue(Holder<Attribute> attribute) {
        return 0.0D;
    }
}
