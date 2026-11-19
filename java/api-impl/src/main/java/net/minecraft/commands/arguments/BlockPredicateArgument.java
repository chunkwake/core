package net.minecraft.commands.arguments;

import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.arguments.ArgumentType;

/** NMS {@code BlockPredicateArgument}: заглушка аргумента Brigadier (Paper-маппер). */
public class BlockPredicateArgument implements ArgumentType<Object> {
    public BlockPredicateArgument() {
    }
    @Override
    public Object parse(StringReader reader) throws com.mojang.brigadier.exceptions.CommandSyntaxException {
        return null;
    }
}
