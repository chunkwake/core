package net.minecraft.server.packs.resources;

/** NMS {@code ResourceProvider}. */
public interface ResourceProvider {
    default java.util.Optional<Resource> getResource(net.minecraft.resources.Identifier id) {
        return java.util.Optional.empty();
    }
}
