package net.minecraft.world.level.block.state.properties;

import java.util.List;
import java.util.Optional;

/** NMS {@code BooleanProperty}. */
public final class BooleanProperty extends Property<Boolean> {
    public static final BooleanProperty TRUE = null;

    private BooleanProperty(String name) {
        super(name, Boolean.class, List.of(Boolean.TRUE, Boolean.FALSE));
    }

    public static BooleanProperty create(String name) {
        return new BooleanProperty(name);
    }

    @Override
    public Optional<Boolean> parseValue(String value) {
        if ("true".equalsIgnoreCase(value)) {
            return Optional.of(Boolean.TRUE);
        }
        if ("false".equalsIgnoreCase(value)) {
            return Optional.of(Boolean.FALSE);
        }
        return Optional.empty();
    }
}
