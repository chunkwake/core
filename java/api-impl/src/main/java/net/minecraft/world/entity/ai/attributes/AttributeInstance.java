package net.minecraft.world.entity.ai.attributes;

/** NMS {@code AttributeInstance}. */
public class AttributeInstance {
    private double baseValue;
    /** Поле, которое ищет CraftEngine (getCachedValue). */
    private double cachedValue;

    public AttributeInstance() {
    }

    public AttributeInstance(net.minecraft.core.Holder<Attribute> attribute,
            java.util.function.Consumer<AttributeInstance> onDirty) {
    }

    public double getValue() {
        return this.baseValue;
    }

    public void setBaseValue(double value) {
        this.baseValue = value;
    }

    public double getBaseValue() {
        return this.baseValue;
    }
}
