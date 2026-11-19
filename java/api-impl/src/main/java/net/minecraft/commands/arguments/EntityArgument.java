package net.minecraft.commands.arguments;

import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.arguments.ArgumentType;

/** NMS {@code EntityArgument}: заглушка аргумента Brigadier (Paper-маппер). */
public class EntityArgument implements ArgumentType<Object> {
    public EntityArgument(boolean single, boolean playersOnly) {
    }

    public static EntityArgument players() {
        return new EntityArgument(false, true);
    }

    public static EntityArgument player() {
        return new EntityArgument(true, true);
    }

    public static EntityArgument entities() {
        return new EntityArgument(false, false);
    }

    public static EntityArgument entity() {
        return new EntityArgument(true, false);
    }
    @Override
    public Object parse(StringReader reader) throws com.mojang.brigadier.exceptions.CommandSyntaxException {
        return null;
    }
}
