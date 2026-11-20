package net.minecraft.util.datafix;

import com.mojang.datafixers.DataFixer;

/** Старое имя {@code DataConverterRegistry} (nightcore). */
public final class DataConverterRegistry {
    private DataConverterRegistry() {
    }

    public static DataFixer getDataFixer() {
        return NoopDataFixer.INSTANCE;
    }
}
