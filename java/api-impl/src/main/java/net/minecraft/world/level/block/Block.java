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

    public Block(String name) {
        this.name = name == null ? "minecraft:air" : name;
        this.defaultBlockState = new BlockState(this);
    }

    /** Заполняет состояния блока (вызывается бутстрапом). */
    public void defineStates(List<BlockState> states, BlockState defaultState) {
        this.stateDefinition = new StateDefinition<>(this, List.of(), states);
        if (defaultState != null) {
            this.defaultBlockState = defaultState;
        }
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
