package org.bukkit.craftbukkit.event;

/** CraftBukkit {@code CraftEventFactory} (CraftEngine). */
public final class CraftEventFactory {
    private CraftEventFactory() {
    }

    public static Object callInventoryOpenEvent(Object player, Object menu) {
        return menu;
    }

    public static boolean handleBlockGrowEvent(Object world, Object pos, Object state, int flags) {
        return false;
    }

    public static boolean handleBlockGrowEvent(Object world, Object pos, Object state) {
        return false;
    }

    public static boolean handleBlockFormEvent(Object world, Object pos, Object state, int flags) {
        return false;
    }

    public static boolean callEntityChangeBlockEvent(Object entity, Object pos, Object state) {
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
