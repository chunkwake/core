package net.minecraft.util.datafix;

import com.mojang.datafixers.DataFixer;

/** NMS {@code DataFixers} (nightcore ищет класс по имени). */
public final class DataFixers {
    private DataFixers() {
    }

    public static DataFixer getDataFixer() {
        return NoopDataFixer.INSTANCE;
    }
}
