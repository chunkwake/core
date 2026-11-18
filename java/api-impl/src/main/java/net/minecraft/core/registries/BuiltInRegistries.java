package net.minecraft.core.registries;

import net.minecraft.core.DefaultedRegistry;
import net.minecraft.core.SimpleDefaultedRegistry;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.level.block.Block;

/** NMS {@code BuiltInRegistries}: базовые реестры. */
public final class BuiltInRegistries {
    public static final DefaultedRegistry<Block> BLOCK = new SimpleDefaultedRegistry<>(
            net.minecraft.resources.Identifier.parse("minecraft:air"));
    public static final DefaultedRegistry<EntityType<?>> ENTITY_TYPE =
            new SimpleDefaultedRegistry<>(
                    net.minecraft.resources.Identifier.parse("minecraft:pig"));

    private BuiltInRegistries() {
    }
}
