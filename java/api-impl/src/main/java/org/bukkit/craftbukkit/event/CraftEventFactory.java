package org.bukkit.craftbukkit.event;

/** CraftBukkit {@code CraftEventFactory} (CraftEngine). */
public final class CraftEventFactory {
    private CraftEventFactory() {
    }

    public static Object callInventoryOpenEvent(Object player, Object menu) {
        return menu;
    }

    public static boolean handleBlockGrowEvent(net.minecraft.world.level.Level world,
            net.minecraft.core.BlockPos pos, net.minecraft.world.level.block.state.BlockState state,
            int flags) {
        return false;
    }

    public static boolean handleBlockGrowEvent(net.minecraft.world.level.Level world,
            net.minecraft.core.BlockPos pos,
            net.minecraft.world.level.block.state.BlockState state) {
        return false;
    }

    public static boolean handleBlockFormEvent(net.minecraft.world.level.Level world,
            net.minecraft.core.BlockPos pos, net.minecraft.world.level.block.state.BlockState state,
            int flags) {
        return false;
    }

    public static boolean callEntityChangeBlockEvent(net.minecraft.world.entity.Entity entity,
            net.minecraft.core.BlockPos pos,
            net.minecraft.world.level.block.state.BlockState state) {
        return false;
    }

    public static org.bukkit.event.block.BlockRedstoneEvent callRedstoneChange$0(
            Object world, Object pos, int oldCurrent, int newCurrent) {
        return null;
    }

    public static org.bukkit.event.block.BlockRedstoneEvent callRedstoneChange$1(
            Object world, Object pos, int oldCurrent, int newCurrent) {
        return null;
    }
}
