package net.minecraft.server.level;

import java.util.List;
import java.util.function.Predicate;
import java.util.concurrent.atomic.AtomicInteger;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.RegistryAccess;
import net.minecraft.world.attribute.EnvironmentAttributeReader;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.flag.FeatureFlagSet;
import net.minecraft.world.level.CommonLevelAccessor;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.BiomeManager;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.dimension.DimensionType;
import net.minecraft.world.level.entity.EntityTypeTest;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.AABB;

/**
 * NMS {@code ServerLevel}. Реализует {@code CommonLevelAccessor}, потому что
 * этого требуют плагины вроде squaremap: они берут его из
 * {@code CraftWorld.getHandle()} и читают чанки/блоки.
 *
 * <p>Мир и чанки живут в Rust. Здесь — прослойка: блоки отдаются заглушкой
 * (воздух), чанков нет. Этого достаточно, чтобы плагин прошёл проверки
 * уровня; для настоящего рендера карт нужен мост chunk/block -> NMS.
 */
public class ServerLevel extends net.minecraft.world.level.Level implements CommonLevelAccessor {
    public static final AtomicInteger ENTITY_COUNTER = new AtomicInteger();

    /** Paper-поле, которое читает squaremap ({@code PaperMapWorld}). */
    public io.papermc.paper.world.saveddata.PaperLevelOverrides serverLevelData =
            new io.papermc.paper.world.saveddata.PaperLevelOverrides();

    private final net.minecraft.world.level.border.WorldBorder worldBorder =
            new net.minecraft.world.level.border.WorldBorder();

    public net.minecraft.world.level.border.WorldBorder getWorldBorder() {
        return this.worldBorder;
    }

    private final ServerChunkCache chunkSource = new ServerChunkCache();
    private final BiomeManager biomeManager = new BiomeManager();
    private final DimensionType dimensionType = new DimensionType();
    /** Поле, которое ищет CraftEngine (getDimension). */
    private net.minecraft.resources.ResourceKey<?> dimension;
    /** Поле, которое ищет CraftEngine (ServerLevelProxy#getStructureManager). */
    /** {@code ServerLevel.getEntityLookup()} (CraftEngine). */
    public net.minecraft.world.level.entity.LevelEntityGetter<net.minecraft.world.entity.Entity> getEntityLookup() {
        return null;
    }

    /** {@code ServerLevel.sendBlockUpdated(...)} (CraftEngine). */
    public void sendBlockUpdated(net.minecraft.core.BlockPos pos,
            net.minecraft.world.level.block.state.BlockState oldState,
            net.minecraft.world.level.block.state.BlockState newState, int flags) {
    }

    private final net.minecraft.world.level.levelgen.structure.StructureManager structureManager =
            new net.minecraft.world.level.levelgen.structure.StructureManager();
    private final FeatureFlagSet features = FeatureFlagSet.of();
    private final EnvironmentAttributeReader attributes = new EnvironmentAttributeReader();

    private volatile org.bukkit.craftbukkit.CraftWorld world;
    private volatile String dimensionKey = "minecraft:overworld";

    /** Ключ измерения для запросов в Rust (`@minecraft:the_nether ...`). */
    public void setDimension(String key) {
        this.dimensionKey = key;
    }

    /** Привязка заглушки к реальному API-миру (WorldApi). */
    public void setWorld(org.bukkit.craftbukkit.CraftWorld world) {
        this.world = world;
    }

    /** {@code ServerLevel.getWorld()}: API-мир (squaremap). */
    public org.bukkit.craftbukkit.CraftWorld getWorld() {
        return this.world;
    }

    private final net.minecraft.world.level.chunk.PalettedContainerFactory containerFactory =
            new net.minecraft.world.level.chunk.PalettedContainerFactory();

    /** {@code ServerLevel.palettedContainerFactory()} (squaremap). */
    public net.minecraft.world.level.chunk.PalettedContainerFactory palettedContainerFactory() {
        return this.containerFactory;
    }

    /** {@code ServerLevel.getChunkSource()} (Orebfuscator). */
    public ServerChunkCache getChunkSource() {
        return this.chunkSource;
    }

    /**
     * {@code ServerLevel.dimension()}: ключ измерения. squaremap читает его
     * для конфигурации миров.
     */
    public net.minecraft.resources.ResourceKey<net.minecraft.world.level.Level> dimension() {
        return net.minecraft.resources.ResourceKey.entryKey(
                net.minecraft.resources.Identifier.parse(this.dimensionKey));
    }

    // --- CommonLevelAccessor / LevelReader ---

    public net.minecraft.world.level.storage.SavedDataStorage getDataStorage() {
        return new net.minecraft.world.level.storage.SavedDataStorage();
    }

    private final ca.spottedleaf.moonrise.patches.chunk_system.scheduling.ChunkTaskScheduler
            moonriseChunkTaskScheduler =
            new ca.spottedleaf.moonrise.patches.chunk_system.scheduling.ChunkTaskScheduler(this);

    public ca.spottedleaf.moonrise.patches.chunk_system.scheduling.ChunkTaskScheduler
            moonrise$getChunkTaskScheduler() {
        return this.moonriseChunkTaskScheduler;
    }

    public net.minecraft.server.MinecraftServer getServer() {
        return net.minecraft.server.MinecraftServer.getServer();
    }

    public ChunkAccess getChunk(int x, int z, ChunkStatus status, boolean load) {
        return new net.minecraft.world.level.chunk.LevelChunk(this, x, z);
    }

    @Override
    public boolean hasChunk(int x, int z) {
        return false;
    }

    @Override
    public int getHeight(Heightmap.Types type, int x, int z) {
        return 0;
    }

    @Override
    public int getSkyDarken() {
        return 15;
    }

    @Override
    public BiomeManager getBiomeManager() {
        return this.biomeManager;
    }

    @Override
    public Holder<Biome> getUncachedNoiseBiome(int x, int y, int z) {
        return null;
    }

    @Override
    public boolean isClientSide() {
        return false;
    }

    @Override
    public int getSeaLevel() {
        return 64;
    }

    @Override
    public DimensionType dimensionType() {
        return this.dimensionType;
    }

    @Override
    public RegistryAccess registryAccess() {
        return RegistryAccess.EMPTY_REGISTRY_ACCESS();
    }

    @Override
    public FeatureFlagSet enabledFeatures() {
        return this.features;
    }

    @Override
    public EnvironmentAttributeReader environmentAttributes() {
        return this.attributes;
    }

    // --- EntityGetter ---

    @Override
    public List<Entity> getEntities(Entity except, AABB box, Predicate<? super Entity> predicate) {
        return List.of();
    }

    @Override
    public <T extends Entity> List<T> getEntities(EntityTypeTest<Entity, T> test, AABB box,
            Predicate<? super T> predicate) {
        return List.of();
    }

    @Override
    public List<? extends Player> players() {
        return List.of();
    }

    /** Блоки: настоящий state id из Rust-мира через мост. */
    @Override
    public net.minecraft.world.level.block.state.BlockState getBlockState(BlockPos pos) {
        // Сначала Java-оверлей (CraftEngine place), затем реальное состояние Rust.
        net.minecraft.world.level.block.state.BlockState overlay = super.getBlockState(pos);
        if (!overlay.isAir()) {
            return overlay;
        }
        // После рестарта custom-блок есть в CustomBlockStore, но не в оверлее.
        // Восстанавливаем его до возврата состояния, чтобы CE isCustomBlock
        // видел DelegatingBlockState, а не воздух из Rust.
        dev.chunkwake.bridge.WorldImpl world = dev.chunkwake.bridge.WorldImpl.of(
                (org.bukkit.World) this.getWorld());
        if (world != null) {
            net.minecraft.world.level.block.state.BlockState restored = world.restoreCustomBlock(pos);
            if (restored != null && !restored.isAir()) {
                return restored;
            }
        }
        return blockStateAt(this.dimensionKey, pos.getX(), pos.getY(), pos.getZ());
    }

    /** Состояние только из Java-оверлея, без запроса в Rust и без restore. */
    public net.minecraft.world.level.block.state.BlockState overlayState(BlockPos pos) {
        return super.getBlockState(pos);
    }

    /** Запрос state id у Rust: `@dim state x y z` -> число. */
    public static net.minecraft.world.level.block.state.BlockState blockStateAt(
            String dim, int x, int y, int z) {
        String answer = dev.chunkwake.bridge.RustWorld.request(
                "@" + dim + " state " + x + " " + y + " " + z);
        int id = 0;
        try {
            id = Integer.parseInt(answer.trim());
        } catch (NumberFormatException ignored) {
            // пусто/ошибка — воздух
        }
        return net.minecraft.world.level.block.Block.stateById(id);
    }
}
