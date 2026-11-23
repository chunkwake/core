package net.minecraft.server.packs.resources;

import java.util.List;
import net.minecraft.server.packs.PackResources;
import net.minecraft.server.packs.PackType;

/**
 * NMS {@code MultiPackResourceManager}: минимальный стаб для CraftEngine
 * data pack proxy. Ресурсы Chunkwake не читает через NMS ResourceManager,
 * поэтому списки пустые; важно, чтобы класс и конструктор существовали.
 */
public class MultiPackResourceManager implements ResourceManager, AutoCloseable {
    private final PackType type;
    private final List<PackResources> packs;

    public MultiPackResourceManager(PackType type, List<PackResources> packs) {
        this.type = type;
        this.packs = packs;
    }

    public PackType getType() {
        return this.type;
    }

    public List<PackResources> getPacks() {
        return this.packs;
    }

    @Override
    public void close() {
    }
}
