package net.minecraft.commands.arguments;

import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.arguments.ArgumentType;

/** NMS {@code ResourceLocationArgument}: заглушка аргумента Brigadier (Paper-маппер). */
public class ResourceLocationArgument implements ArgumentType<Object> {
    public ResourceLocationArgument() {
    }
    @Override
    public Object parse(StringReader reader) throws com.mojang.brigadier.exceptions.CommandSyntaxException {
        return null;
    }
}
