package net.minecraft.network.chat;

import java.util.ArrayList;
import java.util.List;
import net.minecraft.network.chat.contents.objects.ObjectInfo;

/** Изменяемый NMS-компонент: стиль и вложенные siblings. */
public class MutableComponent extends Component {
    private final List<Component> siblings = new ArrayList<>();

    public MutableComponent(String text) {
        this.text = text == null ? "" : text;
    }

    public MutableComponent(ObjectInfo object) {
        this.object = object;
        this.text = "";
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
