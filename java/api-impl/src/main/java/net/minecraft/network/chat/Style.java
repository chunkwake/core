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
    /** Поле, которое ищет CraftEngine (StyleProxy#getHoverEvent). */
    private HoverEvent hoverEvent;

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

    /** NMS {@code Style.withColor(int)}: legacy 0xRRGGBB. */
    public Style withColor(int rgb) {
        return this.withColor(TextColor.fromRgb(rgb));
    }

    public Style withColor(net.minecraft.ChatFormatting formatting) {
        return this.withColor(TextColor.fromLegacyFormat(formatting));
    }

    /** NMS {@code Style.applyFormats} (DecentHolograms). */
    public Style applyFormats(net.minecraft.ChatFormatting... formats) {
        Style style = this;
        if (formats == null) {
            return style;
        }
        for (net.minecraft.ChatFormatting format : formats) {
            if (format == null) {
                continue;
            }
            if (format.isColor()) {
                style = style.withColor(format);
                continue;
            }
            style = switch (format) {
                case BOLD -> style.withBold(true);
                case ITALIC -> style.withItalic(true);
                case UNDERLINE -> style.withUnderlined(true);
                case STRIKETHROUGH -> style.withStrikethrough(true);
                case OBFUSCATED -> style.withObfuscated(true);
                default -> style;
            };
        }
        return style;
    }

    /** NMS {@code Style.applyFormat}. */
    public Style applyFormat(net.minecraft.ChatFormatting format) {
        return this.applyFormats(format);
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
