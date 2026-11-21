package org.bukkit.craftbukkit.block.data;

/** CraftBukkit-заглушка {@code CraftBlockData}: CommandAPI проверяет наличие класса. */
public class CraftBlockData {
    public CraftBlockData() {
    }

    /** {@code CraftBlockData.getState(BlockData)} (CraftEngine). */
    public net.minecraft.world.level.block.state.BlockState getState(
            org.bukkit.block.data.BlockData data) {
        return null;
    }

    /** {@code CraftBlockData.fromData(BlockState)} (CraftEngine). */
    public org.bukkit.block.data.BlockData fromData(
            net.minecraft.world.level.block.state.BlockState state) {
        return null;
    }
}
