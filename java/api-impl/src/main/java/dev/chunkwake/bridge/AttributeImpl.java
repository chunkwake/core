package dev.chunkwake.bridge;

import org.bukkit.NamespacedKey;
import org.bukkit.attribute.Attribute;

/** Реализация Paper {@code Attribute} (интерфейс с 26.3). */
final class AttributeImpl implements Attribute {
    private final NamespacedKey key;
    private final int ordinal;
    private final double defaultValue;

    AttributeImpl(NamespacedKey key, int ordinal, double defaultValue) {
        this.key = key;
        this.ordinal = ordinal;
        this.defaultValue = defaultValue;
    }

    @Override
    public NamespacedKey getKey() {
        return this.key;
    }

    @Override
    public net.kyori.adventure.key.Key key() {
        return this.key;
    }

    @Override
    public String getTranslationKey() {
        return this.translationKey();
    }

    /** {@code net.kyori.adventure.translation.Translatable}. */
    @Override
    public String translationKey() {
        return "attribute.name." + this.key.getKey();
    }

    @Override
    public double getDefaultValue() {
        return this.defaultValue;
    }

    @Override
    public Sentiment getSentiment() {
        return Sentiment.POSITIVE;
    }

    @Override
    public int compareTo(Attribute other) {
        return other == null ? 1 : Integer.compare(this.ordinal, other.ordinal());
    }

    @Override
    public int ordinal() {
        return this.ordinal;
    }

    @Override
    public String name() {
        return this.key.getKey().toUpperCase(java.util.Locale.ROOT);
    }

    @Override
    public String toString() {
        return "Attribute[" + this.key + "]";
    }
}
