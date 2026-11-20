package net.minecraft.util.datafix;

import com.mojang.datafixers.DataFixer;
import com.mojang.datafixers.DSL;
import com.mojang.datafixers.schemas.Schema;
import com.mojang.serialization.Dynamic;

/**
 * Тождественный {@link DataFixer}: сервер поддерживает только 26.3, поэтому
 * апгрейдов данных (DFU) нет — {@code update} возвращает вход как есть.
 * Нужен nightcore: он дергает {@code updateItemStack} и падает на null.
 */
public final class NoopDataFixer implements DataFixer {
    public static final NoopDataFixer INSTANCE = new NoopDataFixer();

    private NoopDataFixer() {
    }

    @Override
    public <T> Dynamic<T> update(DSL.TypeReference type, Dynamic<T> input,
            int version, int newVersion) {
        return input;
    }

    @Override
    public Schema getSchema(int key) {
        return null;
    }
}
