package net.minecraft.network.chat;

import net.minecraft.network.chat.contents.objects.ObjectInfo;

/** NMS {@code Component} — интерфейс (как в 26.x). */
public interface Component {
    String getString();

    Style getStyle();

    ObjectInfo objectInfo();

    static MutableComponent literal(String text) {
        return new MutableComponent(text);
    }

    static MutableComponent translatable(String key) {
        return new MutableComponent(key);
    }

    static MutableComponent keybind(String keybind) {
        return new MutableComponent(keybind);
    }

    static MutableComponent object(ObjectInfo info) {
        return new MutableComponent(info);
    }

    static MutableComponent empty() {
        return new MutableComponent("");
    }

    /** DFU-кодек JSON-компонента (`{"text": ...}`) поверх JsonOps. */
    final class Serializer {
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
