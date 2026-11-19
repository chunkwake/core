package net.minecraft.commands.arguments;

import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.arguments.ArgumentType;

/** NMS {@code ItemPredicateArgument}: заглушка аргумента Brigadier (Paper-маппер). */
public class ItemPredicateArgument implements ArgumentType<Object> {
    public ItemPredicateArgument() {
    }
    @Override
    public Object parse(StringReader reader) throws com.mojang.brigadier.exceptions.CommandSyntaxException {
        return null;
    }
}
