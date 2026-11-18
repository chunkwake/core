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
        /**
         * DFU-кодек JSON-компонента (`{"text": "..."}`) поверх JsonOps.
         * Его использует ProtocolLib WrappedChatComponent (CODEC-поле).
         */
        public static final com.mojang.serialization.Codec<Component> CODEC =
                new com.mojang.serialization.Codec<Component>() {
                    @Override
                    public <T> com.mojang.serialization.DataResult<
                            com.mojang.datafixers.util.Pair<Component, T>> decode(
                            com.mojang.serialization.DynamicOps<T> ops, T input) {
                        com.mojang.serialization.DataResult<
                                com.mojang.serialization.MapLike<T>> map = ops.getMap(input);
                        if (!map.isSuccess()) {
                            return com.mojang.serialization.DataResult.error(
                                    () -> "Not a chat component: " + input);
                        }
                        com.mojang.serialization.MapLike<T> like = map.result().orElse(null);
                        if (like == null) {
                            return com.mojang.serialization.DataResult.error(
                                    () -> "Not a chat component: " + input);
                        }
                        T textTag = like.get("text");
                        if (textTag != null) {
                            com.mojang.serialization.DataResult<String> text =
                                    ops.getStringValue(textTag);
                            if (text.isSuccess()) {
                                return com.mojang.serialization.DataResult.success(
                                        com.mojang.datafixers.util.Pair.of(
                                                Component.literal(text.result().orElse("")), input));
                            }
                        }
                        T translateTag = like.get("translate");
                        if (translateTag != null) {
                            com.mojang.serialization.DataResult<String> key =
                                    ops.getStringValue(translateTag);
                            if (key.isSuccess()) {
                                return com.mojang.serialization.DataResult.success(
                                        com.mojang.datafixers.util.Pair.of(
                                                Component.translatable(key.result().orElse("")),
                                                input));
                            }
                        }
                        return com.mojang.serialization.DataResult.success(
                                com.mojang.datafixers.util.Pair.of(Component.literal(""), input));
                    }

                    @Override
                    public <T> com.mojang.serialization.DataResult<T> encode(Component input,
                            com.mojang.serialization.DynamicOps<T> ops, T prefix) {
                        com.mojang.serialization.RecordBuilder<T> builder = ops.mapBuilder();
                        builder.add("text", ops.createString(input.getString()));
                        return builder.build(prefix);
                    }
                };

        private Serializer() {
        }
    }
}
