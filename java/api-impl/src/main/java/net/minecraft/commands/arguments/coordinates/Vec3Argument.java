package net.minecraft.commands.arguments.coordinates;

import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.arguments.ArgumentType;

/** NMS {@code Vec3Argument} в пакете coordinates (Paper Brigadier). */
public class Vec3Argument implements ArgumentType<Object> {
    public Vec3Argument() {
    }

    public static Vec3Argument vec3() {
        return new Vec3Argument();
    }

    public static Vec3Argument vec3(boolean centerIntegers) {
        return new Vec3Argument();
    }

    @Override
    public Object parse(StringReader reader) {
        return null;
    }
}
