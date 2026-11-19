package net.minecraft.server.level;

import java.util.concurrent.atomic.AtomicInteger;

/**
 * Минимальный NMS {@code ServerLevel}: DecentHolograms берёт из него
 * статический {@code ENTITY_COUNTER} для выдачи entity id.
 */
public class ServerLevel extends net.minecraft.world.level.Level {
    public static final AtomicInteger ENTITY_COUNTER = new AtomicInteger();

    private final ServerChunkCache chunkSource = new ServerChunkCache();

    /** {@code ServerLevel.getChunkSource()} (Orebfuscator). */
    public ServerChunkCache getChunkSource() {
        return this.chunkSource;
    }

    /**
     * {@code ServerLevel.dimension()}: ключ измерения. squaremap читает его
     * для конфигурации миров; отдаём overworld-ключ (карта всё равно не
     * рендерится — ServerLevel у нас заглушка).
     */
    private volatile org.bukkit.craftbukkit.CraftWorld world;

    /** Привязка заглушки к реальному API-миру (WorldApi). */
    public void setWorld(org.bukkit.craftbukkit.CraftWorld world) {
        this.world = world;
    }

    /** {@code ServerLevel.getWorld()}: API-мир (squaremap). */
    public org.bukkit.craftbukkit.CraftWorld getWorld() {
        return this.world;
    }

    public net.minecraft.resources.ResourceKey<net.minecraft.world.level.Level> dimension() {
        return net.minecraft.resources.ResourceKey.entryKey(
                net.minecraft.resources.Identifier.parse("minecraft:overworld"));
    }
}
