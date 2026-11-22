package net.minecraft.core.registries;

import net.minecraft.core.DefaultedRegistry;
import net.minecraft.core.SimpleDefaultedRegistry;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.level.block.Block;

/** NMS {@code BuiltInRegistries}: базовые реестры. */
public final class BuiltInRegistries {
    /** Поле, которое ищет CraftEngine (BuiltInRegistriesProxy#getAttribute). */
    public static final net.minecraft.core.Registry<Object> attribute =
            new net.minecraft.core.SimpleRegistry<>();
    public static final net.minecraft.core.Registry<Object> ATTRIBUTE = attribute;
    public static final net.minecraft.core.Registry<Object> item =
            new net.minecraft.core.SimpleRegistry<>();
    public static final net.minecraft.core.Registry<Object> ITEM = item;
    public static final net.minecraft.core.Registry<Object> dataComponentType =
            new net.minecraft.core.SimpleRegistry<>();
    public static final net.minecraft.core.Registry<Object> DATA_COMPONENT_TYPE = dataComponentType;
    public static final net.minecraft.core.Registry<Object> feature =
            new net.minecraft.core.SimpleRegistry<>();
    public static final net.minecraft.core.Registry<Object> FEATURE = feature;
    public static final net.minecraft.core.Registry<Object> fluid =
            new net.minecraft.core.SimpleRegistry<>();
    public static final net.minecraft.core.Registry<Object> FLUID = fluid;
    public static final net.minecraft.core.Registry<Object> recipeType =
            new net.minecraft.core.SimpleRegistry<>();
    public static final net.minecraft.core.Registry<Object> RECIPE_TYPE = recipeType;
    public static final net.minecraft.core.Registry<Object> blockstateProviderType =
            new net.minecraft.core.SimpleRegistry<>();
    public static final net.minecraft.core.Registry<Object> BLOCKSTATE_PROVIDER_TYPE =
            blockstateProviderType;
    public static final net.minecraft.core.Registry<Object> particleType =
            new net.minecraft.core.SimpleRegistry<>();
    public static final net.minecraft.core.Registry<Object> PARTICLE_TYPE = particleType;
    public static final net.minecraft.core.Registry<Object> soundEvent =
            new net.minecraft.core.SimpleRegistry<>();
    public static final net.minecraft.core.Registry<Object> SOUND_EVENT = soundEvent;
    public static final net.minecraft.core.Registry<Object> BLOCK_ENTITY_TYPE =
            new net.minecraft.core.SimpleRegistry<>();
    public static final net.minecraft.core.Registry<Object> DATA_COMPONENT_PREDICATE_TYPE =
            new net.minecraft.core.SimpleRegistry<>();
    public static final net.minecraft.core.Registry<Object> GAME_EVENT =
            new net.minecraft.core.SimpleRegistry<>();
    public static final net.minecraft.core.Registry<Object> LOOT_POOL_ENTRY_TYPE =
            new net.minecraft.core.SimpleRegistry<>();
    public static final net.minecraft.core.Registry<Object> MOB_EFFECT =
            new net.minecraft.core.SimpleRegistry<>();

    public static final DefaultedRegistry<Block> BLOCK = new SimpleDefaultedRegistry<>(
            net.minecraft.resources.Identifier.parse("minecraft:air"));
    public static final DefaultedRegistry<EntityType<?>> ENTITY_TYPE =
            new SimpleDefaultedRegistry<>(
                    net.minecraft.resources.Identifier.parse("minecraft:pig"));

    /** Реестры жителей (FancyNpcs: {@code VILLAGER_PROFESSION}). */
    public static final DefaultedRegistry<Object> VILLAGER_PROFESSION =
            new SimpleDefaultedRegistry<>(
                    net.minecraft.resources.Identifier.parse("minecraft:none"));

    public static final DefaultedRegistry<Object> VILLAGER_TYPE =
            new SimpleDefaultedRegistry<>(
                    net.minecraft.resources.Identifier.parse("minecraft:plains"));



    /**
     * Реестр реестров ({@code BuiltInRegistries.REGISTRY}). PacketEvents/
     * Incendo Cloud достают через него {@code command_argument_type}.
     */
    public static final net.minecraft.core.Registry<net.minecraft.core.Registry<?>> REGISTRY =
            new net.minecraft.core.SimpleRegistry<>();

    /** Аргументы Brigadier (ключ → info); используется Cloud-маппером. */
    public static final net.minecraft.core.Registry<Object> COMMAND_ARGUMENT_TYPE =
            new SimpleDefaultedRegistry<>(net.minecraft.resources.Identifier.parse("minecraft:uuid"));

    private static void argument(String key, Class<?> type) {
        Object info = new Object();
        COMMAND_ARGUMENT_TYPE.register(net.minecraft.resources.Identifier.parse("minecraft:" + key),
                info);
        net.minecraft.commands.synchronization.ArgumentTypeInfos.register(type, info);
    }

    static {
        ITEM.register(net.minecraft.resources.Identifier.parse("minecraft:bedrock"),
                new net.minecraft.world.item.Item("minecraft:bedrock"));
        ITEM.register(net.minecraft.resources.Identifier.parse("minecraft:air"),
                new net.minecraft.world.item.Item("minecraft:air"));
        REGISTRY.register(net.minecraft.resources.Identifier.parse("minecraft:command_argument_type"),
                COMMAND_ARGUMENT_TYPE);
        argument("uuid", net.minecraft.commands.arguments.UuidArgument.class);
        argument("resource_location",
                net.minecraft.commands.arguments.ResourceLocationArgument.class);
        argument("item_enchantment",
                net.minecraft.commands.arguments.ItemEnchantmentArgument.class);
        argument("item_stack", net.minecraft.commands.arguments.ItemArgument.class);
        argument("item_predicate",
                net.minecraft.commands.arguments.ItemPredicateArgument.class);
        argument("block_predicate",
                net.minecraft.commands.arguments.BlockPredicateArgument.class);
        argument("entity", net.minecraft.commands.arguments.EntityArgument.class);
        argument("vec3", net.minecraft.commands.arguments.Vec3Argument.class);
        argument("vec2", net.minecraft.commands.arguments.Vec2Argument.class);
        argument("resource_key", net.minecraft.commands.arguments.ResourceKeyArgument.class);
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
