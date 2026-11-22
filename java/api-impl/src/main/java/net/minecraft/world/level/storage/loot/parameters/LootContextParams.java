package net.minecraft.world.level.storage.loot.parameters;

import net.minecraft.util.context.ContextKey;

/** NMS {@code LootContextParams}: ключи параметров лут-контекста. */
public final class LootContextParams {
    public static final ContextKey<Object> THIS_ENTITY = new ContextKey<>("this_entity");
    public static final ContextKey<Object> INTERACTING_ENTITY = new ContextKey<>("interacting_entity");
    public static final ContextKey<Object> TARGET_ENTITY = new ContextKey<>("target_entity");
    public static final ContextKey<Object> LAST_DAMAGE_PLAYER = new ContextKey<>("last_damage_player");
    public static final ContextKey<Object> DAMAGE_SOURCE = new ContextKey<>("damage_source");
    public static final ContextKey<Object> ATTACKING_ENTITY = new ContextKey<>("attacking_entity");
    public static final ContextKey<Object> KILLER_ENTITY = new ContextKey<>("killer_entity");
    public static final ContextKey<Object> DIRECT_ATTACKING_ENTITY = new ContextKey<>("direct_attacking_entity");
    public static final ContextKey<Object> DIRECT_KILLER_ENTITY = new ContextKey<>("direct_killer_entity");
    public static final ContextKey<Object> ORIGIN = new ContextKey<>("origin");
    public static final ContextKey<Object> BLOCK_STATE = new ContextKey<>("block_state");
    public static final ContextKey<Object> BLOCK_ENTITY = new ContextKey<>("block_entity");
    public static final ContextKey<Object> TOOL = new ContextKey<>("tool");
    public static final ContextKey<Object> EXPLOSION_RADIUS = new ContextKey<>("explosion_radius");
    public static final ContextKey<Object> LOOTING_MOD = new ContextKey<>("looting_mod");
    public static final ContextKey<Object> ENCHANTMENT_LEVEL = new ContextKey<>("enchantment_level");
    public static final ContextKey<Object> ENCHANTMENT_ACTIVE = new ContextKey<>("enchantment_active");

    private LootContextParams() {
    }
}
