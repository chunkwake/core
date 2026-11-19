package net.minecraft.world.level.block.state;

import java.util.Collection;
import java.util.List;
import net.minecraft.world.level.block.state.properties.Property;

/** NMS {@code StateDefinition}. */
public class StateDefinition<O, S> {
    private final O owner;
    private final Collection<Property<?>> properties;
    private final Collection<S> states;

    public StateDefinition(O owner, Collection<Property<?>> properties) {
        this(owner, properties, List.of());
    }

    public StateDefinition(O owner, Collection<Property<?>> properties, Collection<S> states) {
        this.owner = owner;
        this.properties = List.copyOf(properties);
        this.states = List.copyOf(states);
    }

    public O getOwner() {
        return this.owner;
    }

    public Collection<Property<?>> getProperties() {
        return this.properties;
    }

    public Property<?> getProperty(String name) {
        return this.properties.stream()
                .filter(p -> p.getName().equals(name))
                .findFirst()
                .orElse(null);
    }

    public com.google.common.collect.ImmutableList<S> getPossibleStates() {
        return com.google.common.collect.ImmutableList.copyOf(this.states);
    }
}
