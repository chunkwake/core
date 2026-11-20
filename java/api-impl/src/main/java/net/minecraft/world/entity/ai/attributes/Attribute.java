package net.minecraft.world.entity.ai.attributes;

/** NMS {@code Attribute}. */
public class Attribute {
    private final double defaultValue;

    public Attribute(double defaultValue) {
        this.defaultValue = defaultValue;
    }

    public double getDefaultValue() {
        return this.defaultValue;
    }
}
