package net.minecraft.commands.arguments;

import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.arguments.ArgumentType;

/** NMS {@code ItemEnchantmentArgument}: заглушка аргумента Brigadier (Paper-маппер). */
public class ItemEnchantmentArgument implements ArgumentType<Object> {
    public ItemEnchantmentArgument() {
    }
    @Override
    public Object parse(StringReader reader) throws com.mojang.brigadier.exceptions.CommandSyntaxException {
        return null;
    }
}
