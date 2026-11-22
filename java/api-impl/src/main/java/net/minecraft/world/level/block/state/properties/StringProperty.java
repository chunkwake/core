package net.minecraft.world.level.block.state.properties;

import java.util.List;
import java.util.Optional;

/**
 * Строковое свойство состояния (enum-подобное в ваниле).
 *
 * <p>Реальные enum-классы Minecraft (Direction, Half, Instrument...) для
 * Java-слоя не нужны: CraftEngine читает значения через {@code PropertyProxy}
 * и сравнивает их {@code toString()}, поэтому строковое представление
 * полностью совместимо и не требует генерации сотен enum-типов.
 */
public final class StringProperty extends Property<String> {
    private StringProperty(String name, List<String> values) {
        super(name, String.class, values);
    }

    public static StringProperty create(String name, List<String> values) {
        return new StringProperty(name, values);
    }

    @Override
    public Optional<String> parseValue(String value) {
        return this.getPossibleValues().stream()
                .filter(v -> v.equalsIgnoreCase(value))
                .findFirst();
    }
}
