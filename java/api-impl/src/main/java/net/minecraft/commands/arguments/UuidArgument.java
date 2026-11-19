package net.minecraft.commands.arguments;

import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.arguments.ArgumentType;

/** NMS {@code UuidArgument}: заглушка аргумента Brigadier (Paper-маппер). */
public class UuidArgument implements ArgumentType<Object> {
    public UuidArgument() {
    }

    public static UuidArgument uuid() {
        return new UuidArgument();
    }
    @Override
    public Object parse(StringReader reader) throws com.mojang.brigadier.exceptions.CommandSyntaxException {
        return null;
    }
}
