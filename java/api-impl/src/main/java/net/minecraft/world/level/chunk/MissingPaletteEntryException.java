package net.minecraft.world.level.chunk;

/** Заглушка NMS MissingPaletteEntryException (squaremap: ServerLevel). */
public class MissingPaletteEntryException extends RuntimeException {
    public MissingPaletteEntryException(int id) {
        super("Missing palette entry for id " + id);
    }
}
