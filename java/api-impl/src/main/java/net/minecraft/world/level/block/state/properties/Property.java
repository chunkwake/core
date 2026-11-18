package net.minecraft.world.level.block.state.properties;

import java.util.Collection;
import java.util.List;
import java.util.Optional;

/** NMS {@code Property}: свойство состояния блока. */
public abstract class Property<T extends Comparable<T>> {
    private final String name;
    private final Class<T> type;
    private final List<T> values;

    protected Property(String name, Class<T> type, List<T> values) {
        this.name = name;
        this.type = type;
        this.values = List.copyOf(values);
    }

    public String getName() {
        return this.name;
    }

    public Class<T> getValueClass() {
        return this.type;
    }

    public Collection<T> getPossibleValues() {
        return this.values;
    }

    public List<T> getPossibleValuesList() {
        return this.values;
    }

    public Optional<T> getValue(String value) {
        return this.values.stream().filter(v -> v.toString().equals(value)).findFirst();
    }

    public abstract Optional<T> parseValue(String value);

    @Override
    public String toString() {
        return "Property[" + this.name + "]";
    }
}
