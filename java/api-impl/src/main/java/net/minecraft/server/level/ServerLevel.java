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

    private final ServerChunkCache chunkSource = new ServerChunkCache();
    private final BiomeManager biomeManager = new BiomeManager();
    private final DimensionType dimensionType = new DimensionType();
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

    @Override
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
        return blockStateAt(this.dimensionKey, pos.getX(), pos.getY(), pos.getZ());
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
