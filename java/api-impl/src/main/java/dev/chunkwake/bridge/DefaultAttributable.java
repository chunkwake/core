package dev.chunkwake.bridge;

import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Map;
import org.bukkit.attribute.Attribute;
import org.bukkit.attribute.AttributeInstance;
import org.bukkit.attribute.AttributeModifier;
import org.bukkit.attribute.Attributable;

/** Снимок ванильных атрибутов сущности ({@code EntityType.getDefaultAttributes()}). */
final class DefaultAttributable implements Attributable {
    private final Map<Attribute, Double> values;

    DefaultAttributable(Map<Attribute, Double> values) {
        this.values = values;
    }

    @Override
    public AttributeInstance getAttribute(Attribute attribute) {
        Double base = this.values.get(attribute);
        return base == null ? null : new DefaultAttributeInstance(attribute, base);
    }

    @Override
    public void registerAttribute(Attribute attribute) {
        // Снимок не меняется: неизвестный атрибут добавляем с 0.
        this.values.putIfAbsent(attribute, 0.0);
    }
}

/** Изменяемый снимок значения атрибута (модификаторы локальны). */
final class DefaultAttributeInstance implements AttributeInstance {
    private final Attribute attribute;
    private final List<AttributeModifier> modifiers = new ArrayList<>();
    private double base;

    DefaultAttributeInstance(Attribute attribute, double base) {
        this.attribute = attribute;
        this.base = base;
    }

    @Override public Attribute getAttribute() { return this.attribute; }
    @Override public double getBaseValue() { return this.base; }
    @Override public double getDefaultValue() { return this.base; }

    @Override
    public void setBaseValue(double value) {
        if (!Double.isFinite(value)) {
            throw new IllegalArgumentException("Base value must be finite");
        }
        this.base = value;
    }

    @Override
    public double getValue() {
        double value = this.base;
        for (AttributeModifier modifier : this.modifiers) {
            if (modifier.getOperation() == AttributeModifier.Operation.ADD_NUMBER) {
                value += modifier.getAmount();
            }
        }
        for (AttributeModifier modifier : this.modifiers) {
            if (modifier.getOperation() == AttributeModifier.Operation.ADD_SCALAR) {
                value += this.base * modifier.getAmount();
            }
        }
        for (AttributeModifier modifier : this.modifiers) {
            if (modifier.getOperation() == AttributeModifier.Operation.MULTIPLY_SCALAR_1) {
                value *= 1.0 + modifier.getAmount();
            }
        }
        return value;
    }

    @Override
    public Collection<AttributeModifier> getModifiers() {
        return List.copyOf(this.modifiers);
    }

    @Override
    public AttributeModifier getModifier(net.kyori.adventure.key.Key key) {
        for (AttributeModifier modifier : this.modifiers) {
            if (modifier.getKey().equals(key)) {
                return modifier;
            }
        }
        return null;
    }

    @Override
    @SuppressWarnings("deprecation")
    public AttributeModifier getModifier(java.util.UUID uuid) {
        return getModifier(org.bukkit.NamespacedKey.minecraft(uuid.toString()));
    }

    @Override
    public void removeModifier(net.kyori.adventure.key.Key key) {
        this.modifiers.removeIf(modifier -> modifier.getKey().equals(key));
    }

    @Override
    @SuppressWarnings("deprecation")
    public void removeModifier(java.util.UUID uuid) {
        removeModifier(org.bukkit.NamespacedKey.minecraft(uuid.toString()));
    }

    @Override
    public void addModifier(AttributeModifier modifier) {
        if (modifier == null) {
            throw new IllegalArgumentException("modifier");
        }
        if (getModifier(modifier.getKey()) != null) {
            throw new IllegalArgumentException("Modifier is already applied on this attribute!");
        }
        this.modifiers.add(modifier);
    }

    @Override
    public void addTransientModifier(AttributeModifier modifier) {
        addModifier(modifier);
    }

    @Override
    public void removeModifier(AttributeModifier modifier) {
        if (modifier == null) {
            throw new IllegalArgumentException("modifier");
        }
        removeModifier(modifier.getKey());
    }

    @Override
    public String toString() {
        return "DefaultAttributeInstance{" + this.attribute.getKey() + "=" + this.base + "}";
    }
}
