package dev.chunkwake.bridge;

/**
 * Провод {@code Player.playEffect(Location, Effect, int)}:
 * {@code player <uuid> effect <level-event> <x> <y> <z> <data>}.
 *
 * <p>Ваниль шлёт {@code ClientboundLevelEventPacket} только этому игроку;
 * Rust собирает такой же пакет из этой строки.
 */
final class EffectWire {
    private EffectWire() {
    }

    static String command(org.bukkit.Location location, org.bukkit.Effect effect, int data) {
        return "effect " + effect.getId()
                + " " + location.getX()
                + " " + location.getY()
                + " " + location.getZ()
                + " " + data;
    }
}
