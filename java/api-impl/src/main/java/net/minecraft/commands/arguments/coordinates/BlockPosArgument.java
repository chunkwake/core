package net.minecraft.commands.arguments.coordinates;

import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.arguments.ArgumentType;
import com.mojang.brigadier.exceptions.CommandSyntaxException;

/**
 * NMS {@code BlockPosArgument}: аргумент Brigadier «координаты блока».
 * Нужен Lamp/revxrsal (FancyHolograms и др.), который ищет класс по имени
 * и вызывает приватный конструктор без аргументов.
 */
public class BlockPosArgument implements ArgumentType<Object> {
    private BlockPosArgument() {
    }

    public static BlockPosArgument blockPos() {
        return new BlockPosArgument();
    }

    @Override
    public Object parse(StringReader reader) throws CommandSyntaxException {
        while (reader.canRead() && !Character.isWhitespace(reader.peek())) {
            reader.skip();
        }
        return null;
    }
}
