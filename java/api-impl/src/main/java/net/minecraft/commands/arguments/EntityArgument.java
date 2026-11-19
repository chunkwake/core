package net.minecraft.commands.arguments;

import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.arguments.ArgumentType;

/** NMS {@code EntityArgument}: заглушка аргумента Brigadier (Paper-маппер). */
public class EntityArgument implements ArgumentType<Object> {
    public EntityArgument(boolean single, boolean playersOnly) {
    }
    @Override
    public Object parse(StringReader reader) throws com.mojang.brigadier.exceptions.CommandSyntaxException {
        return null;
    }
}
