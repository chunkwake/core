package net.minecraft.network.chat;

import net.minecraft.network.chat.contents.objects.ObjectInfo;

/** Минимальный NMS-компонент текста с методами, которые использует TAB. */
public class Component {
    protected String text = "";
    protected Style style = Style.EMPTY;
    protected ObjectInfo object;

    public Component() {
    }

    public static MutableComponent literal(String text) {
        return new MutableComponent(text);
    }

    public static MutableComponent translatable(String key) {
        return new MutableComponent(key);
    }

    public static MutableComponent keybind(String keybind) {
        return new MutableComponent(keybind);
    }

    public static MutableComponent object(ObjectInfo info) {
        return new MutableComponent(info);
    }

    public String getString() {
        return this.text;
    }

    public Style getStyle() {
        return this.style;
    }

    public ObjectInfo objectInfo() {
        return this.object;
    }

    public static class Serializer {
        /** Настоящий DFU-кодек: строка ↔ компонент (как plain-text fallback). */
        public static final com.mojang.serialization.Codec<Component> CODEC =
                com.mojang.serialization.Codec.STRING.xmap(
                        Component::literal, Component::getString);

        private Serializer() {
        }
    }
}
