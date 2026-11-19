package net.minecraft.commands.arguments;

import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.arguments.ArgumentType;

/** NMS DimensionArgument (Paper Brigadier): нужен squaremap. */
public class DimensionArgument implements ArgumentType<Object> {
    public DimensionArgument() {
    }

    public static DimensionArgument dimension() {
        return new DimensionArgument();
    }

    @Override
    public Object parse(StringReader reader) {
        return null;
    }
}
