package io.papermc.paper.command.brigadier;

import com.mojang.brigadier.Message;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer;

/**
 * Paper {@code MessageComponentSerializer}: мост Component <-> Brigadier
 * Message. Регистрируется через ServiceLoader (CommandAPI зовёт
 * {@code MessageComponentSerializer.message()}).
 */
public final class ChunkwakeMessageComponentSerializer implements MessageComponentSerializer {
    private static final PlainTextComponentSerializer PLAIN =
            PlainTextComponentSerializer.plainText();

    public ChunkwakeMessageComponentSerializer() {
    }

    @Override
    public Message serialize(Component component) {
        if (component == null) {
            return () -> "";
        }
        return () -> PLAIN.serialize(component);
    }

    @Override
    public Component deserialize(Message message) {
        if (message == null) {
            return Component.empty();
        }
        return Component.text(message.getString());
    }
}
