package net.minecraft;

/** NMS {@code ChatFormatting}. */
public enum ChatFormatting {
    BLACK('0'), DARK_BLUE('1'), DARK_GREEN('2'), DARK_AQUA('3'),
    DARK_RED('4'), DARK_PURPLE('5'), GOLD('6'), GRAY('7'),
    DARK_GRAY('8'), BLUE('9'), GREEN('a'), AQUA('b'),
    RED('c'), LIGHT_PURPLE('d'), YELLOW('e'), WHITE('f'),
    OBFUSCATED('k'), BOLD('l'), STRIKETHROUGH('m'), UNDERLINE('n'),
    ITALIC('o'), RESET('r');

    private final char code;

    ChatFormatting(char code) {
        this.code = code;
    }

    public char getChar() {
        return this.code;
    }

    public boolean isColor() {
        return this.ordinal() < 16;
    }

    public boolean isFormat() {
        return this.ordinal() >= 16 && this != RESET;
    }

    public static ChatFormatting getByCode(char code) {
        for (ChatFormatting value : values()) {
            if (value.code == code) {
                return value;
            }
        }
        return null;
    }
}
