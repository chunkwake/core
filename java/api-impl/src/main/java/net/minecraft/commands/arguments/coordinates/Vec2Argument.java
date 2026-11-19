package net.minecraft.commands.arguments.coordinates;

import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.arguments.ArgumentType;

/** NMS {@code Vec2Argument} в пакете coordinates (Paper Brigadier). */
public class Vec2Argument implements ArgumentType<Object> {
    public Vec2Argument() {
    }

    public static Vec2Argument vec2() {
        return new Vec2Argument();
    }

    public static Vec2Argument vec2(boolean centerIntegers) {
        return new Vec2Argument();
    }

    @Override
    public Object parse(StringReader reader) {
        return null;
    }
}
