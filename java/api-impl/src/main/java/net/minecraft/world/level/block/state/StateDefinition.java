package net.minecraft.world.level.block.state;

import java.util.Collection;
import java.util.List;
import net.minecraft.world.level.block.state.properties.Property;

/** NMS {@code StateDefinition}. */
public class StateDefinition<O, S> {
    private final O owner;
    private final Collection<Property<?>> properties;

    public StateDefinition(O owner, Collection<Property<?>> properties) {
        this.owner = owner;
        this.properties = List.copyOf(properties);
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

    public Collection<S> getPossibleStates() {
        return List.of();
    }
}
