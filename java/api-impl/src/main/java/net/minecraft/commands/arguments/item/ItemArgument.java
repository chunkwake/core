package net.minecraft.commands.arguments.item;

import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.arguments.ArgumentType;

/** NMS ItemArgument (item package). */
public class ItemArgument implements ArgumentType<Object> {
    public ItemArgument() {}
    public static ItemArgument item(net.minecraft.commands.CommandBuildContext ctx) { return new ItemArgument(); }
    public static ItemArgument item(net.minecraft.commands.CommandBuildContext ctx, boolean allowTags) { return new ItemArgument(); }
    @Override public Object parse(StringReader reader) { return null; }
}
