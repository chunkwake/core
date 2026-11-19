package net.minecraft.commands.arguments;

import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.arguments.ArgumentType;

/** NMS {@code Vec3Argument}: заглушка аргумента Brigadier (Paper-маппер). */
public class Vec3Argument implements ArgumentType<Object> {
    public Vec3Argument(boolean center) {
    }
    @Override
    public Object parse(StringReader reader) throws com.mojang.brigadier.exceptions.CommandSyntaxException {
        return null;
    }
}
