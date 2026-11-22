package net.minecraft.resources;

import java.util.Map;

/** NMS {@code FileToIdConverter}. */
public class FileToIdConverter {
    private final String prefix;
    private final String extension;

    public FileToIdConverter(String prefix, String extension) {
        this.prefix = prefix;
        this.extension = extension;
    }

    public static FileToIdConverter json(String prefix) {
        return new FileToIdConverter(prefix, ".json");
    }

    public static FileToIdConverter registry(ResourceKey<?> registry) {
        return new FileToIdConverter("", ".json");
    }

    public Identifier fileToId(Identifier file) {
        return file;
    }

    public Identifier idToFile(Identifier id) {
        return id;
    }

    public Map<Object, Object> listMatchingResources(
            net.minecraft.server.packs.resources.ResourceManager resourceManager) {
        return Map.of();
    }

    public Map<Object, java.util.List<Object>> listMatchingResourceStacks(
            net.minecraft.server.packs.resources.ResourceManager resourceManager) {
        return Map.of();
    }
}
