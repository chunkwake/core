package net.minecraft.world.entity.ai.attributes;

import java.util.Collections;
import java.util.Map;

/** NMS {@code AttributeSupplier}: набор базовых атрибутов. */
public class AttributeSupplier {
    public static final AttributeSupplier EMPTY = new AttributeSupplier(Collections.emptyMap());

    private final Map<Attribute, Double> values;

    public AttributeSupplier(Map<Attribute, Double> values) {
        this.values = values == null ? Collections.emptyMap() : values;
    }

    /** {@code AttributeSupplier.getBaseValue(Holder)} (CraftEngine). */
    public double getBaseValue(net.minecraft.core.Holder<Attribute> attribute) {
        return attribute == null ? 0.0 : this.getValue(attribute.value());
    }

    /** {@code AttributeSupplier.hasAttribute(Holder)} (CraftEngine). */
    public boolean hasAttribute(net.minecraft.core.Holder<Attribute> attribute) {
        return attribute != null && this.hasAttribute(attribute.value());
    }

    public boolean hasAttribute(Attribute attribute) {
        return this.values.containsKey(attribute);
    }

    public double getValue(Attribute attribute) {
        return this.values.getOrDefault(attribute, attribute == null ? 0.0D
                : attribute.getDefaultValue());
    }

    public Map<Attribute, Double> getAttributes() {
        return this.values;
    }
}
