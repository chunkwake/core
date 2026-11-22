package net.minecraft.world.level.block.state;

import java.util.Collection;
import java.util.List;
import net.minecraft.world.level.block.state.properties.Property;

/** NMS {@code StateDefinition}. */
public class StateDefinition<O, S> {
    private final O owner;
    private final Collection<Property<?>> properties;
    private final com.google.common.collect.ImmutableList<S> states;

    public StateDefinition(O owner, Collection<Property<?>> properties) {
        this(owner, properties, List.of());
    }

    public StateDefinition(O owner, Collection<Property<?>> properties, Collection<S> states) {
        this.owner = owner;
        this.properties = List.copyOf(properties);
        this.states = com.google.common.collect.ImmutableList.copyOf(states);
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
        return this.states;
    }

    /** NMS {@code StateDefinition.Builder}. */
    public static class Builder<O, S> {
        private final O owner;
        private final java.util.List<Property<?>> properties = new java.util.ArrayList<>();

        public Builder(O owner) {
            this.owner = owner;
        }

        public Builder<O, S> add(Property<?>... properties) {
            java.util.Collections.addAll(this.properties, properties);
            return this;
        }

        public StateDefinition<O, S> create(java.util.function.Function<O, S> factory,
                Factory<O, S> stateFactory) {
            Property<?>[] keys = this.properties.toArray(new Property<?>[0]);
            Comparable<?>[] values = new Comparable<?>[keys.length];
            S state = stateFactory.create(this.owner, keys, values);
            if (state == null) {
                state = factory.apply(this.owner);
            }
            return new StateDefinition<>(this.owner, this.properties,
                    state == null ? java.util.List.of() : java.util.List.of(state));
        }
    }

    /** NMS {@code StateDefinition.Factory}. */
    public interface Factory<O, S> {
        S create(O owner, Property<?>[] properties, Comparable<?>[] values);
    }
}
