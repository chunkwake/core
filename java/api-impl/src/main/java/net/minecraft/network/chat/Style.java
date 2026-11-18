package net.minecraft.network.chat;

/** Минимальный NMS {@code Style} с теми же with-методами, что у ванильного. */
public final class Style {
    public static final Style EMPTY = new Style();

    private final TextColor color;
    private final Boolean bold;
    private final Boolean italic;
    private final Boolean underlined;
    private final Boolean strikethrough;
    private final Boolean obfuscated;
    private final FontDescription font;
    private final Integer shadowColor;

    private Style() {
        this(null, null, null, null, null, null, null, null);
    }

    private Style(TextColor color, Boolean bold, Boolean italic, Boolean underlined,
            Boolean strikethrough, Boolean obfuscated, FontDescription font,
            Integer shadowColor) {
        this.color = color;
        this.bold = bold;
        this.italic = italic;
        this.underlined = underlined;
        this.strikethrough = strikethrough;
        this.obfuscated = obfuscated;
        this.font = font;
        this.shadowColor = shadowColor;
    }

    public Style withColor(net.minecraft.ChatFormatting formatting) {
        return this.withColor(TextColor.fromLegacyFormat(formatting));
    }

    public Style withColor(TextColor color) {
        return new Style(color, bold, italic, underlined, strikethrough, obfuscated, font, shadowColor);
    }

    public Style withBold(Boolean value) {
        return new Style(color, value, italic, underlined, strikethrough, obfuscated, font, shadowColor);
    }

    public Style withItalic(Boolean value) {
        return new Style(color, bold, value, underlined, strikethrough, obfuscated, font, shadowColor);
    }

    public Style withUnderlined(Boolean value) {
        return new Style(color, bold, italic, value, strikethrough, obfuscated, font, shadowColor);
    }

    public Style withStrikethrough(Boolean value) {
        return new Style(color, bold, italic, underlined, value, obfuscated, font, shadowColor);
    }

    public Style withObfuscated(Boolean value) {
        return new Style(color, bold, italic, underlined, strikethrough, value, font, shadowColor);
    }

    public Style withFont(FontDescription font) {
        return new Style(color, bold, italic, underlined, strikethrough, obfuscated, font, shadowColor);
    }

    public Style withShadowColor(Integer shadowColor) {
        return new Style(color, bold, italic, underlined, strikethrough, obfuscated, font, shadowColor);
    }

    public TextColor getColor() { return this.color; }
    public Boolean getBold() { return this.bold; }
    public Boolean getItalic() { return this.italic; }
    public Boolean getUnderlined() { return this.underlined; }
    public Boolean getStrikethrough() { return this.strikethrough; }
    public Boolean getObfuscated() { return this.obfuscated; }
    public FontDescription getFont() { return this.font; }
    public Integer getShadowColor() { return this.shadowColor; }
}
