package net.minecraft.world.level.block;

import java.util.List;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.StateDefinition;

/**
 * NMS {@code Block}. Реестр состояний наполняется из настоящего
 * {@code reports/blocks.json} (id состояний = сетевые id), поэтому
 * плагины (Orebfuscator, Axiom) видят ванильные блоки и их состояния.
 */
public class Block extends net.minecraft.world.level.block.state.BlockBehaviour {
    public static final net.minecraft.core.IdMapper<BlockState> BLOCK_STATE_REGISTRY =
            new net.minecraft.core.IdMapper<>();

    private final String name;
    private BlockState defaultBlockState;
    protected StateDefinition<Block, BlockState> stateDefinition;
    /** Все состояния блока по каноническому ключу свойств. */
    private final java.util.Map<String, BlockState> statesByKey = new java.util.HashMap<>();
    private Object builtInRegistryHolder;
    // Поля BlockBehaviour, но объявленные здесь: ProxyBinder (CraftEngine)
    // ищет их через getDeclaredField именно у Block.
    protected boolean hasCollision = true;
    protected float explosionResistance;
    protected boolean isRandomlyTicking;
    protected Object soundType;
    protected float friction = 0.6F;
    protected float speedFactor = 1.0F;
    protected float jumpFactor = 1.0F;
    protected float bounceRestitution;
    protected String descriptionId = "";

    public Block() {
        this("minecraft:air");
    }

    public Block(net.minecraft.world.level.block.state.BlockBehaviour.Properties properties) {
        super(properties);
        this.name = properties != null && properties.id != null
                ? properties.id.location().toString() : "minecraft:air";
        this.defaultBlockState = new BlockState(this);
    }

    public Block(String name) {
        this.name = name == null ? "minecraft:air" : name;
        this.defaultBlockState = new BlockState(this);
    }

    /** Заполняет состояния блока (вызывается бутстрапом). */
    public void defineStates(List<BlockState> states, BlockState defaultState) {
        defineStates(states, defaultState, List.of());
    }

    /** Заполняет состояния и свойства блока (вызывается BlockBootstrap). */
    public void defineStates(List<BlockState> states, BlockState defaultState,
            java.util.Collection<net.minecraft.world.level.block.state.properties.Property<?>> properties) {
        this.stateDefinition = new StateDefinition<>(this, properties == null ? List.of() : properties, states);
        if (defaultState != null) {
            this.defaultBlockState = defaultState;
        }
        this.statesByKey.clear();
        if (states != null) {
            for (BlockState state : states) {
                if (state != null) {
                    this.statesByKey.put(stateKey(state.getValues()), state);
                }
            }
        }
    }

    /** Состояние по значениям свойств; {@code null}, если комбинации нет. */
    public BlockState stateFor(java.util.Map<net.minecraft.world.level.block.state.properties.Property<?>, Comparable<?>> values) {
        return this.statesByKey.get(stateKey(values));
    }

    /** Канонический ключ: свойства сортируются по имени. */
    public static String stateKey(
            java.util.Map<net.minecraft.world.level.block.state.properties.Property<?>, Comparable<?>> values) {
        if (values == null || values.isEmpty()) {
            return "";
        }
        java.util.List<java.util.Map.Entry<net.minecraft.world.level.block.state.properties.Property<?>, Comparable<?>>> entries =
                new java.util.ArrayList<>(values.entrySet());
        entries.sort(java.util.Comparator.comparing(entry -> entry.getKey().getName()));
        StringBuilder out = new StringBuilder();
        for (java.util.Map.Entry<net.minecraft.world.level.block.state.properties.Property<?>, Comparable<?>> entry : entries) {
            out.append(entry.getKey().getName()).append('=')
                    .append(entry.getValue().toString().toLowerCase(java.util.Locale.ROOT)).append(';');
        }
        return out.toString();
    }

    public BlockState defaultBlockState() {
        return this.defaultBlockState;
    }

    /** {@code Block.defaultMapColor()} (squaremap). */
    public net.minecraft.world.level.material.MapColor defaultMapColor() {
        return net.minecraft.world.level.material.MapColor.NONE;
    }

    public String name() {
        return this.name;
    }

    /** {@code Block.builtInRegistryHolder} (CraftEngine BlockProxy). */
    public Object getBuiltInRegistryHolder() {
        return this.builtInRegistryHolder;
    }

    public void setBuiltInRegistryHolder(Object holder) {
        this.builtInRegistryHolder = holder;
    }

    /** Идентификатор блока: holder (custom/carrier), иначе name. */
    public String id() {
        Object holder = this.builtInRegistryHolder;
        if (holder != null) {
            try {
                Object key = holder.getClass().getMethod("key").invoke(holder);
                if (key != null) {
                    Object location = key.getClass().getMethod("location").invoke(key);
                    if (location != null) {
                        return location.toString();
                    }
                }
            } catch (Throwable ignored) {
                // fallback на name
            }
            try {
                String registered = (String) holder.getClass().getMethod("getRegisteredName").invoke(holder);
                if (registered != null && !registered.isEmpty()) {
                    return registered;
                }
            } catch (Throwable ignored) {
                // fallback на name
            }
        }
        return this.name;
    }

    public static int getId(BlockState state) {
        return BLOCK_STATE_REGISTRY.getId(state);
    }

    public static BlockState stateById(int id) {
        return BLOCK_STATE_REGISTRY.byId(id);
    }

    public StateDefinition<Block, BlockState> getStateDefinition() {
        return this.stateDefinition != null ? this.stateDefinition
                : new StateDefinition<>(this, List.of());
    }

    /** {@code Block.asItem()}. */
    public net.minecraft.world.item.Item asItem() {
        return null;
    }

    /** {@code Block.getStateForPlacement(BlockPlaceContext)}. */
    public BlockState getStateForPlacement(
            net.minecraft.world.item.context.BlockPlaceContext context) {
        return this.defaultBlockState();
    }

    /** {@code Block.dropResources(BlockState, Level, BlockPos)}. */
    public static void dropResources(BlockState state, net.minecraft.world.level.Level level,
            net.minecraft.core.BlockPos pos) {
    }

    /** {@code Block.canSupportCenter(LevelReader, BlockPos, Direction)}. */
    public static boolean canSupportCenter(net.minecraft.world.level.LevelReader level,
            net.minecraft.core.BlockPos pos, net.minecraft.core.Direction direction) {
        return true;
    }

    /** {@code Block.canSupportRigidBlock(BlockGetter, BlockPos)}. */
    public static boolean canSupportRigidBlock(net.minecraft.world.level.BlockGetter level,
            net.minecraft.core.BlockPos pos) {
        return true;
    }

    public String getDescriptionId() {
        return "block." + this.name.replace(':', '.');
    }

    public String getName() {
        String path = this.name.contains(":")
                ? this.name.substring(this.name.indexOf(':') + 1) : this.name;
        return path;
    }

    public void stepOn(net.minecraft.world.level.Level level, net.minecraft.core.BlockPos pos,
            BlockState state, net.minecraft.world.entity.Entity entity) {
    }

    /** Paper 1.20.3+: возвращает состояние после разрушения игроком. */
    public BlockState playerWillDestroy(net.minecraft.world.level.Level level,
            net.minecraft.core.BlockPos pos, BlockState state,
            net.minecraft.world.entity.player.Player player) {
        return state;
    }

    public void fallOn(net.minecraft.world.level.Level level, BlockState state,
            net.minecraft.core.BlockPos pos, net.minecraft.world.entity.Entity entity, double fallDistance) {
    }

    public void setPlacedBy(net.minecraft.world.level.Level level, net.minecraft.core.BlockPos pos,
            BlockState state, net.minecraft.world.entity.LivingEntity placer,
            net.minecraft.world.item.ItemStack stack) {
    }

    public void handlePrecipitation(BlockState state, net.minecraft.world.level.Level level,
            net.minecraft.core.BlockPos pos, net.minecraft.world.level.biome.Biome.Precipitation precipitation) {
    }

    @Override
    public String toString() {
        return this.name;
    }
}
