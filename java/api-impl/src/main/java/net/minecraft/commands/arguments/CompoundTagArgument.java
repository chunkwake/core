package net.minecraft.commands.arguments;

import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.arguments.ArgumentType;

/** NMS CompoundTagArgument (Paper Brigadier). */
public class CompoundTagArgument implements ArgumentType<Object> {
    public CompoundTagArgument() {
    }

    public static CompoundTagArgument compoundTag() {
        return new CompoundTagArgument();
    }

    public static CompoundTagArgument tag(net.minecraft.commands.CommandBuildContext ctx) {
        return new CompoundTagArgument();
    }

    @Override
    public Object parse(StringReader reader) {
        return null;
    }
}
