package net.minecraft.data.registries;

import net.minecraft.core.HolderLookup;

/**
 * NMS {@code VanillaRegistries}: поставщик ванильных реестров.
 *
 * <p>NpcPlugin на 26.3 зовёт {@code createWorldLookup()} рефлексией и
 * передаёт результат в {@code Commands.createValidationContext}. Реестры
 * уже подняты {@code VanillaBootstrap}, поэтому заглушка отдаёт пустой
 * {@link HolderLookup.Provider} — для построения дерева команд этого
 * достаточно.
 */
public final class VanillaRegistries {
    private static final HolderLookup.Provider LOOKUP = new HolderLookup.Provider() {
    };

    private VanillaRegistries() {
    }

    public static HolderLookup.Provider createLookup() {
        return LOOKUP;
    }

    public static HolderLookup.Provider createWorldLookup() {
        return LOOKUP;
    }
}
