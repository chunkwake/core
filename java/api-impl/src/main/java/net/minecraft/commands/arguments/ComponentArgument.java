package net.minecraft.commands.arguments;

import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.arguments.ArgumentType;

/**
 * NMS {@code ComponentArgument}: аргумент-компонент (Paper Brigadier).
 *
 * <p>Фабрика {@code textComponent(CommandBuildContext)} есть у NpcPlugin в
 * дереве `/npc ... message`; сам разбор нам не нужен, но тип обязан быть
 * настоящим {@link ArgumentType}, иначе `Commands.argument` отвергнет его.
 */
public class ComponentArgument implements ArgumentType<Object> {
    public ComponentArgument() {
    }

    public static ComponentArgument textComponent(
            net.minecraft.commands.CommandBuildContext context) {
        return new ComponentArgument();
    }

    @Override
    public Object parse(StringReader reader) {
        return null;
    }
}
