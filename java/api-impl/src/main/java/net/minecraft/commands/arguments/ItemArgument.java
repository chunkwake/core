package net.minecraft.commands.arguments;

import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.arguments.ArgumentType;

/** NMS {@code ItemArgument}: заглушка аргумента Brigadier (Paper-маппер). */
public class ItemArgument implements ArgumentType<Object> {
    public ItemArgument() {
    }
    @Override
    public Object parse(StringReader reader) throws com.mojang.brigadier.exceptions.CommandSyntaxException {
        return null;
    }
}
