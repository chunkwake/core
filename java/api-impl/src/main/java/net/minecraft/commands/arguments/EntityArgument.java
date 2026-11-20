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
    public static net.minecraft.world.entity.Entity getEntity(
            com.mojang.brigadier.context.CommandContext<?> context, String name) {
        Object value = context.getArgument(name, Object.class);
        return value instanceof net.minecraft.world.entity.Entity entity ? entity : null;
    }

    @Override
    public Object parse(StringReader reader) throws com.mojang.brigadier.exceptions.CommandSyntaxException {
        return null;
    }
}
