package net.minecraft.server.packs.resources;

import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.Predicate;
import java.util.stream.Stream;
import net.minecraft.resources.Identifier;
import net.minecraft.server.packs.PackResources;

/** NMS {@code ResourceManager}. */
public interface ResourceManager extends ResourceProvider {
    default Set<String> getNamespaces() {
        return Set.of();
    }

    default List<Resource> getResourceStack(Identifier id) {
        return List.of();
    }

    default Map<Identifier, Resource> listResources(String path, Selector selector) {
        return Map.of();
    }

    default Map<Identifier, List<Resource>> listResourceStacks(String path, Selector selector) {
        return Map.of();
    }

    default Map<Object, Object> listResources(String path, Predicate<Object> selector) {
        return Map.of();
    }

    default Map<Object, Object> listResources(String path, Object selector) {
        return Map.of();
    }

    default Stream<PackResources> listPacks() {
        return Stream.empty();
    }

    /** NMS {@code ResourceManager.Selector}. */
    interface Selector {
        boolean test(Identifier id);
    }
}
