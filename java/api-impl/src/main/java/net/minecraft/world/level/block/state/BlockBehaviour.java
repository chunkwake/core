package net.minecraft.world.level.block.state;

/**
 * NMS {@code BlockBehaviour}: общие поля и методы блока.
 *
 * <p>Набор методов повторяет Paper 26.3: CraftEngine через ProxyBinder ищет
 * их через {@code getDeclaredMethod} с точными сигнатурами. Реализация —
 * безопасные значения по умолчанию: Chunkwake не исполняет серверную логику
 * блоков, но API-контракт должен существовать, иначе инициализация плагина
 * падает с {@code ExceptionInInitializerError}.
 */
public class BlockBehaviour {
    protected boolean hasCollision = true;
    protected float explosionResistance;
    protected boolean isRandomlyTicking;
    protected Object soundType;
    protected float friction = 0.6F;
    protected float speedFactor = 1.0F;
    protected float jumpFactor = 1.0F;
    protected float bounceRestitution;
    protected float fallDistanceReduction = 1.0F;
    protected boolean dynamicShape;
    protected Object requiredFeatures;
    protected Object properties;
    protected java.util.Optional<Object> drops = java.util.Optional.empty();
    protected String descriptionId = "";

    public BlockBehaviour() {
    }

    public BlockBehaviour(Properties properties) {
        this.properties = properties;
    }

    protected boolean isPathfindable(BlockState state,
            net.minecraft.world.level.pathfinder.PathComputationType type) {
        return false;
    }

    protected BlockState updateShape(BlockState state,
            net.minecraft.world.level.LevelReader level,
            net.minecraft.world.level.ScheduledTickAccess ticks,
            net.minecraft.core.BlockPos pos,
            net.minecraft.core.Direction direction,
            net.minecraft.core.BlockPos neighborPos,
            BlockState neighborState,
            net.minecraft.util.RandomSource random) {
        return state;
    }

    protected void neighborChanged(BlockState state, net.minecraft.world.level.Level level,
            net.minecraft.core.BlockPos pos, net.minecraft.world.level.block.Block block,
            net.minecraft.world.level.redstone.Orientation orientation, boolean movedByPiston) {
    }

    protected void onPlace(BlockState state, net.minecraft.world.level.Level level,
            net.minecraft.core.BlockPos pos, BlockState oldState, boolean movedByPiston) {
    }

    protected void onPlace(BlockState state, net.minecraft.world.level.Level level,
            net.minecraft.core.BlockPos pos, BlockState oldState, boolean movedByPiston,
            net.minecraft.world.item.context.UseOnContext context) {
    }

    protected void onExplosionHit(BlockState state, net.minecraft.server.level.ServerLevel level,
            net.minecraft.core.BlockPos pos, net.minecraft.world.level.Explosion explosion,
            java.util.function.BiConsumer<net.minecraft.world.item.ItemStack, net.minecraft.core.BlockPos> dropConsumer) {
    }

    protected boolean triggerEvent(BlockState state, net.minecraft.world.level.Level level,
            net.minecraft.core.BlockPos pos, int id, int param) {
        return false;
    }

    protected boolean hasAnalogOutputSignal(BlockState state) {
        return false;
    }

    protected BlockState rotate(BlockState state, net.minecraft.world.level.block.Rotation rotation) {
        return state;
    }

    protected BlockState mirror(BlockState state, net.minecraft.world.level.block.Mirror mirror) {
        return state;
    }

    protected java.util.List<net.minecraft.world.item.ItemStack> getDrops(BlockState state,
            net.minecraft.world.level.storage.loot.LootParams.Builder builder) {
        return java.util.List.of();
    }

    protected net.minecraft.world.phys.shapes.VoxelShape getBlockSupportShape(BlockState state,
            net.minecraft.world.level.BlockGetter level, net.minecraft.core.BlockPos pos) {
        return net.minecraft.world.phys.shapes.VoxelShape.EMPTY;
    }

    protected boolean canSurvive(BlockState state, net.minecraft.world.level.LevelReader level,
            net.minecraft.core.BlockPos pos) {
        return true;
    }

    protected int getAnalogOutputSignal(BlockState state, net.minecraft.world.level.Level level,
            net.minecraft.core.BlockPos pos, net.minecraft.core.Direction direction) {
        return 0;
    }

    protected net.minecraft.world.phys.shapes.VoxelShape getShape(BlockState state,
            net.minecraft.world.level.BlockGetter level, net.minecraft.core.BlockPos pos,
            net.minecraft.world.phys.shapes.CollisionContext context) {
        return net.minecraft.world.phys.shapes.VoxelShape.EMPTY;
    }

    protected net.minecraft.world.phys.shapes.VoxelShape getCollisionShape(BlockState state,
            net.minecraft.world.level.BlockGetter level, net.minecraft.core.BlockPos pos,
            net.minecraft.world.phys.shapes.CollisionContext context) {
        return net.minecraft.world.phys.shapes.VoxelShape.EMPTY;
    }

    protected net.minecraft.world.phys.shapes.VoxelShape getEntityInsideCollisionShape(BlockState state,
            net.minecraft.world.level.BlockGetter level, net.minecraft.core.BlockPos pos,
            net.minecraft.world.entity.Entity entity) {
        return net.minecraft.world.phys.shapes.VoxelShape.EMPTY;
    }

    protected void randomTick(BlockState state, net.minecraft.server.level.ServerLevel level,
            net.minecraft.core.BlockPos pos, net.minecraft.util.RandomSource random) {
    }

    protected void tick(BlockState state, net.minecraft.server.level.ServerLevel level,
            net.minecraft.core.BlockPos pos, net.minecraft.util.RandomSource random) {
    }

    protected void affectNeighborsAfterRemoval(BlockState state,
            net.minecraft.server.level.ServerLevel level, net.minecraft.core.BlockPos pos,
            boolean movedByPiston) {
    }

    protected void spawnAfterBreak(BlockState state, net.minecraft.server.level.ServerLevel level,
            net.minecraft.core.BlockPos pos, net.minecraft.world.item.ItemStack stack, boolean dropExperience) {
    }

    protected void attack(BlockState state, net.minecraft.world.level.Level level,
            net.minecraft.core.BlockPos pos, net.minecraft.world.entity.player.Player player) {
    }

    protected boolean isSignalSource(BlockState state) {
        return false;
    }

    protected int getSignal(BlockState state, net.minecraft.world.level.BlockGetter level,
            net.minecraft.core.BlockPos pos, net.minecraft.core.Direction direction) {
        return 0;
    }

    protected void entityInside(BlockState state, net.minecraft.world.level.Level level,
            net.minecraft.core.BlockPos pos, net.minecraft.world.entity.Entity entity,
            net.minecraft.world.entity.InsideBlockEffectApplier applier, boolean inside) {
    }

    protected int getDirectSignal(BlockState state, net.minecraft.world.level.BlockGetter level,
            net.minecraft.core.BlockPos pos, net.minecraft.core.Direction direction) {
        return 0;
    }

    protected void onProjectileHit(net.minecraft.world.level.Level level, BlockState state,
            net.minecraft.world.phys.BlockHitResult hit,
            net.minecraft.world.entity.projectile.Projectile projectile) {
    }

    protected boolean isRandomlyTicking(BlockState state) {
        return false;
    }

    /** NMS {@code BlockBehaviour.StatePredicate}. */
    public interface StatePredicate {
        boolean test(BlockState state, net.minecraft.world.level.BlockGetter level,
                net.minecraft.core.BlockPos pos);
    }

    /** NMS {@code BlockBehaviour.StateArgumentPredicate}. */
    public interface StateArgumentPredicate<A> {
        boolean test(BlockState state, net.minecraft.world.level.BlockGetter level,
                net.minecraft.core.BlockPos pos, A argument);
    }

    /** NMS {@code BlockBehaviour.PostProcess}. */
    public interface PostProcess {
        net.minecraft.core.BlockPos getPostProcessPos(BlockState state,
                net.minecraft.world.level.BlockGetter level, net.minecraft.core.BlockPos pos);
    }

    /** NMS {@code BlockBehaviour.OffsetFunction}. */
    public interface OffsetFunction {
        net.minecraft.world.phys.Vec3 evaluate(BlockState state, net.minecraft.core.BlockPos pos);
    }

    /** NMS {@code BlockBehaviour.OffsetType}. */
    public enum OffsetType {
        NONE,
        XZ,
        XYZ
    }

    /** NMS {@code BlockBehaviour.Properties}: builder свойств блока. */
    public static class Properties {
        public net.minecraft.resources.ResourceKey<net.minecraft.world.level.block.Block> id;

        private Properties() {
        }

        public static Properties of() {
            return new Properties();
        }

        public Properties setId(net.minecraft.resources.ResourceKey<net.minecraft.world.level.block.Block> id) {
            this.id = id;
            return this;
        }
    }

    /**
     * NMS {@code BlockBehaviour.BlockStateBase}.
     *
     * <p>Реальный {@code BlockState} наследуется от этого класса, а CraftEngine
     * через ProxyBinder берёт {@code BlockStateBaseProxy.CLASS} и ищет
     * <em>объявленные</em> здесь {@code is(Block)} и
     * {@code getDrops(LootParams.Builder)}.
     */
    public static class BlockStateBase extends StateHolder<net.minecraft.world.level.block.Block, BlockState>
            implements net.minecraft.core.TypedInstance<net.minecraft.world.level.block.Block> {
        /**
         * Paper кэширует вычисленные флаги состояния в этом поле;
         * CraftEngine биндит его как {@code BlockStateBaseProxy#getCache}.
         */
        protected BlockStateBase() {
        }

        protected BlockStateBase(net.minecraft.world.level.block.Block owner,
                net.minecraft.world.level.block.state.properties.Property<?>[] keys,
                Comparable<?>[] values) {
        }

        private int lightEmission;
        private boolean useShapeForLightOcclusion;
        private boolean isAir;
        private boolean ignitedByLava;
        private boolean liquid;
        private boolean legacySolid;
        private Object pushReaction;
        private Object mapColor;
        private float destroySpeed;
        private boolean requiresCorrectToolForDrops;
        private boolean canOcclude;
        private Object isRedstoneConductor;
        private Object isSuffocating;
        private Object isViewBlocking;
        private Object postProcess;
        private Object emissiveRendering;
        private Object offsetFunction;
        private Object instrument;
        private boolean replaceable;
        private Object fluidState;
        private boolean isRandomlyTicking;
        private boolean solidRender;
        private Object occlusionShape;
        private boolean propagatesSkylightDown;
        private int lightDampening;
        private boolean conditionallyFullOpaque;
        private int opacityIfCached;
        protected boolean shapeExceedsCube;
        private Cache cache;

        public Cache getCache() {
            return this.cache;
        }

        @Override
        public net.minecraft.core.Holder<net.minecraft.world.level.block.Block> typeHolder() {
            return null;
        }

        @Override
        public java.util.stream.Stream<net.minecraft.tags.TagKey<net.minecraft.world.level.block.Block>> tags() {
            return java.util.stream.Stream.empty();
        }

        public void setCache(Cache cache) {
            this.cache = cache;
        }

        public void initCache() {
        }

        public final boolean moonrise$hasCache() {
            return this.cache != null;
        }

        public net.minecraft.world.level.block.Block getBlock() {
            return null;
        }

        public final boolean is(net.minecraft.world.level.block.Block block) {
            return this.getBlock() == block;
        }

        public final boolean is(net.minecraft.tags.TagKey<net.minecraft.world.level.block.Block> tag) {
            return false;
        }

        public final boolean is(net.minecraft.core.HolderSet<net.minecraft.world.level.block.Block> set) {
            return false;
        }

        public final boolean is(net.minecraft.core.Holder<net.minecraft.world.level.block.Block> holder) {
            return false;
        }

        public final boolean is(net.minecraft.resources.ResourceKey<net.minecraft.world.level.block.Block> key) {
            return false;
        }

        public final boolean is(net.minecraft.tags.TagKey<net.minecraft.world.level.block.Block> tag,
                java.util.function.Predicate<BlockStateBase> predicate) {
            return false;
        }

        public boolean isAir() {
            return false;
        }

        public java.util.List<net.minecraft.world.item.ItemStack> getDrops(
                net.minecraft.world.level.storage.loot.LootParams.Builder builder) {
            return java.util.List.of();
        }

        public int getLightDampening() {
            return this.lightDampening;
        }

        public int getLightBlock(net.minecraft.world.level.BlockGetter level,
                net.minecraft.core.BlockPos pos) {
            return this.lightDampening;
        }

        public void randomTick(net.minecraft.server.level.ServerLevel level,
                net.minecraft.core.BlockPos pos, net.minecraft.util.RandomSource random) {
        }

        public net.minecraft.world.level.block.SoundType getSoundType() {
            return null;
        }

        public boolean isPathfindable(net.minecraft.world.level.pathfinder.PathComputationType type) {
            return false;
        }

        public boolean isPathfindable(net.minecraft.world.level.BlockGetter level,
                net.minecraft.core.BlockPos pos,
                net.minecraft.world.level.pathfinder.PathComputationType type) {
            return false;
        }

        public boolean isFaceSturdy(net.minecraft.world.level.BlockGetter level,
                net.minecraft.core.BlockPos pos, net.minecraft.core.Direction direction,
                net.minecraft.world.level.block.SupportType type) {
            return false;
        }

        public boolean isCollisionShapeFullBlock(net.minecraft.world.level.BlockGetter level,
                net.minecraft.core.BlockPos pos) {
            return false;
        }

        public boolean canSurvive(net.minecraft.world.level.LevelReader level,
                net.minecraft.core.BlockPos pos) {
            return true;
        }

        public boolean isSignalSource() {
            return false;
        }

        public net.minecraft.world.phys.shapes.VoxelShape getShape(
                net.minecraft.world.level.BlockGetter level, net.minecraft.core.BlockPos pos,
                net.minecraft.world.phys.shapes.CollisionContext context) {
            return net.minecraft.world.phys.shapes.VoxelShape.EMPTY;
        }

        public net.minecraft.world.phys.shapes.VoxelShape getCollisionShape(
                net.minecraft.world.level.BlockGetter level, net.minecraft.core.BlockPos pos,
                net.minecraft.world.phys.shapes.CollisionContext context) {
            return net.minecraft.world.phys.shapes.VoxelShape.EMPTY;
        }

        public net.minecraft.world.phys.shapes.VoxelShape getBlockSupportShape(
                net.minecraft.world.level.BlockGetter level, net.minecraft.core.BlockPos pos) {
            return net.minecraft.world.phys.shapes.VoxelShape.EMPTY;
        }

        public void onPlace(net.minecraft.world.level.Level level, net.minecraft.core.BlockPos pos,
                BlockState state, boolean movedByPiston) {
        }

        public float getDestroyProgress(net.minecraft.world.entity.player.Player player,
                net.minecraft.world.level.BlockGetter level, net.minecraft.core.BlockPos pos) {
            return 0.0F;
        }

        public void updateNeighbourShapes(net.minecraft.world.level.LevelAccessor level,
                net.minecraft.core.BlockPos pos, int flags, int recursionLeft) {
        }

        public boolean hasBlockEntity() {
            return false;
        }

        public org.bukkit.block.data.BlockData asBlockData() {
            return dev.chunkwake.bridge.BlockDataImpl.fromState(
                    (net.minecraft.world.level.block.state.BlockState) (Object) this);
        }

        /** NMS {@code BlockBehaviour.BlockStateBase.Cache}. */
        public static class Cache {
            public int lightBlock;
            public boolean propagatesSkylightDown;
        }
    }
}
