package org.bukkit.craftbukkit.block.data;

/**
 * CraftBukkit {@code CraftBlockData} как интерфейс-маркер.
 *
 * <p>Bukkit-совместимость Chunkwake отдаёт {@code BlockData} через
 * динамический прокси; CE кастует его к {@code CraftBlockData} и вызывает
 * {@code getState()}. Поэтому класс заменён интерфейсом, который этот прокси
 * реализует.
 */
public interface CraftBlockData {
    net.minecraft.world.level.block.state.BlockState getState();

    static net.minecraft.world.level.block.state.BlockState getState(org.bukkit.block.data.BlockData data) {
        return data instanceof CraftBlockData craft ? craft.getState() : null;
    }

    static org.bukkit.block.data.BlockData fromData(
            net.minecraft.world.level.block.state.BlockState state) {
        return null;
    }
}
