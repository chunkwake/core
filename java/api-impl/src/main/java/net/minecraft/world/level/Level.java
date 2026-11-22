package net.minecraft.world.level;

import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.state.BlockState;

/** NMS {@code Level} (минимально, но с настоящей иерархией). */
public class Level implements LevelAccessor, net.minecraft.world.level.LevelHeightAccessor,
        net.minecraft.world.level.LevelWriter {
    /**
     * Java-оверлей блоков. CraftEngine ставит custom-состояния через
     * {@code LevelWriter.setBlock}; Bukkit-блоки читают тот же overlay.
     * Истинная геометрия/тик по-прежнему в Rust, но для API-контракта
     * (isCustomBlock/place/getBlockData) нужен видимый NMS-слой.
     */
    private final java.util.Map<net.minecraft.core.BlockPos, BlockState> overlay =
            new java.util.concurrent.ConcurrentHashMap<>();
    /** {@code Level.updateNeighborsAt(BlockPos, Block[, Orientation])} (CraftEngine). */
    public void updateNeighborsAt(net.minecraft.core.BlockPos pos,
            net.minecraft.world.level.block.Block block) {
    }

    public void updateNeighborsAt(net.minecraft.core.BlockPos pos,
            net.minecraft.world.level.block.Block block,
            net.minecraft.world.level.redstone.Orientation orientation) {
    }

    /** Поле, которое ищет CraftEngine (getDimension). */
    private net.minecraft.resources.ResourceKey<?> dimension;

    /** Поле, которое ищет CraftEngine (LevelProxy#getRandom). */
    private net.minecraft.util.RandomSource random;

    /** {@code Level.getWorld()} (CraftEngine). */
    public org.bukkit.craftbukkit.CraftWorld getWorld() {
        return null;
    }

    /** {@code Level.dimensionTypeRegistration} (CraftEngine FieldGetter). */
    private net.minecraft.core.Holder<net.minecraft.world.level.dimension.DimensionType> dimensionTypeRegistration;

    /** {@code Level.dimensionType()} (CraftEngine). */
    public net.minecraft.world.level.dimension.DimensionType dimensionType() {
        return new net.minecraft.world.level.dimension.DimensionType();
    }

    public net.minecraft.core.Holder<net.minecraft.world.level.dimension.DimensionType> dimensionTypeRegistration() {
        return this.dimensionTypeRegistration;
    }

    /** {@code Level.isLoaded(BlockPos)} (CraftEngine). */
    public boolean isLoaded(net.minecraft.core.BlockPos pos) {
        return true;
    }

    public final boolean checkEntityCollision(BlockState state,
            net.minecraft.world.entity.Entity entity,
            net.minecraft.world.phys.shapes.CollisionContext context,
            net.minecraft.core.BlockPos pos, boolean checkCollision) {
        return false;
    }

    public void updateNeighbourForOutputSignal(net.minecraft.core.BlockPos pos,
            net.minecraft.world.level.block.Block block) {
    }

    public void setBlocksDirty(net.minecraft.core.BlockPos pos, BlockState oldState,
            BlockState newState) {
    }

    public Object moonrise$getEntityLookup() {
        return null;
    }

    public boolean removeBlock(net.minecraft.core.BlockPos pos, boolean move) {
        return this.overlay.remove(pos) != null;
    }

    public void removeBlockEntity(net.minecraft.core.BlockPos pos) {
    }


    /** Ключи измерений (squaremap проверяет NETHER/END). */
    public static final net.minecraft.resources.ResourceKey<Level> OVERWORLD =
            net.minecraft.resources.ResourceKey.entryKey(
                    net.minecraft.resources.Identifier.parse("minecraft:overworld"));
    public static final net.minecraft.resources.ResourceKey<Level> NETHER =
            net.minecraft.resources.ResourceKey.entryKey(
                    net.minecraft.resources.Identifier.parse("minecraft:the_nether"));
    public static final net.minecraft.resources.ResourceKey<Level> END =
            net.minecraft.resources.ResourceKey.entryKey(
                    net.minecraft.resources.Identifier.parse("minecraft:the_end"));
    /** Поле нужно ProtocolLib (BukkitConverters ищет CraftWorld в Level). */
    public org.bukkit.craftbukkit.CraftWorld world;

    @Override
    public int getHeight() {
        return 384;
    }

    @Override
    public int getMinY() {
        return -64;
    }

    @Override
    public BlockState getBlockState(BlockPos pos) {
        if (pos == null) {
            return net.minecraft.world.level.block.Blocks.AIR.defaultBlockState();
        }
        BlockState state = this.overlay.get(pos);
        return state == null ? net.minecraft.world.level.block.Blocks.AIR.defaultBlockState() : state;
    }

    @Override
    public boolean setBlock(BlockPos pos, BlockState state, int flags) {
        if (pos == null) {
            return false;
        }
        if (state == null || state.isAir()) {
            this.overlay.remove(pos);
        } else {
            this.overlay.put(pos, state);
        }
        return true;
    }

    @Override
    public boolean setBlock(BlockPos pos, BlockState state, int flags, int recursionLeft) {
        return this.setBlock(pos, state, flags);
    }

    @Override
    public boolean destroyBlock(BlockPos pos, boolean dropBlock,
            net.minecraft.world.entity.Entity entity, int recursionLeft) {
        return true;
    }

    public net.minecraft.core.RegistryAccess registryAccess() {
        return net.minecraft.core.RegistryAccess.EMPTY_REGISTRY_ACCESS();
    }

    public long getGameTime() {
        return 0L;
    }
}
