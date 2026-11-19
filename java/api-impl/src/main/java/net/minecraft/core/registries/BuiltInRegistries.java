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

    static {
        entity("minecraft:pig", 0);
        entity("minecraft:text_display", 135);
        entity("minecraft:item_display", 73);
        entity("minecraft:block_display", 15);
        entity("minecraft:armor_stand", 5);
        entity("minecraft:item", 72);
    }

    private static void entity(String id, int protocolId) {
        ENTITY_TYPE.register(net.minecraft.resources.Identifier.parse(id),
                new EntityType<>(id, protocolId));
    }

    private BuiltInRegistries() {
    }
}
