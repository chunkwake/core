package net.minecraft.network.chat;

import java.util.ArrayList;
import java.util.List;
import net.minecraft.network.chat.contents.objects.ObjectInfo;

/** Изменяемый NMS-компонент: стиль и вложенные siblings. */
public class MutableComponent implements Component {
    private final List<Component> siblings = new ArrayList<>();
    private String text = "";
    private Style style = Style.EMPTY;
    private ObjectInfo object;

    private ComponentContents contents;

    public MutableComponent(String text) {
        this.text = text == null ? "" : text;
    }

    /** NMS {@code MutableComponent.create(ComponentContents)} (DecentHolograms). */
    public static MutableComponent create(ComponentContents contents) {
        MutableComponent component = new MutableComponent("");
        component.contents = contents;
        if (contents instanceof net.minecraft.network.chat.contents.PlainTextContents plain) {
            component.text = plain.text();
        }
        return component;
    }

    public ComponentContents getContents() {
        return this.contents;
    }

    public MutableComponent(ObjectInfo object) {
        this.object = object;
        this.text = "";
    }

    @Override
    public Style getStyle() {
        return this.style;
    }

    @Override
    public ObjectInfo objectInfo() {
        return this.object;
    }

    public MutableComponent withStyle(Style style) {
        return this.setStyle(style);
    }

    public MutableComponent withStyle(net.minecraft.ChatFormatting... formats) {
        Style style = this.getStyle();
        for (net.minecraft.ChatFormatting format : formats) {
            if (format == null) continue;
            if (format.isColor()) {
                style = style.withColor(format);
            } else {
                switch (format) {
                    case BOLD -> style = style.withBold(true);
                    case ITALIC -> style = style.withItalic(true);
                    case UNDERLINE -> style = style.withUnderlined(true);
                    case STRIKETHROUGH -> style = style.withStrikethrough(true);
                    case OBFUSCATED -> style = style.withObfuscated(true);
                    default -> { }
                }
            }
        }
        return this.setStyle(style);
    }

    public MutableComponent setStyle(Style style) {
        this.style = style == null ? Style.EMPTY : style;
        return this;
    }

    public MutableComponent append(Component sibling) {
        if (sibling != null) {
            this.siblings.add(sibling);
        }
        return this;
    }

    public List<Component> getSiblings() {
        return this.siblings;
    }

    @Override
    public String getString() {
        StringBuilder out = new StringBuilder(this.text);
        for (Component sibling : this.siblings) {
            out.append(sibling.getString());
        }
        return out.toString();
    }
}
