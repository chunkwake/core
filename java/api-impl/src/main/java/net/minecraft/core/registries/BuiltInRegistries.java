package net.minecraft.core.registries;

import net.minecraft.core.Registry;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.level.block.Block;

/** NMS {@code BuiltInRegistries}: базовые реестры. */
public final class BuiltInRegistries {
    public static final Registry<Block> BLOCK = new Registry<>();
    public static final Registry<EntityType<?>> ENTITY_TYPE = new Registry<>();

    private BuiltInRegistries() {
    }
}
