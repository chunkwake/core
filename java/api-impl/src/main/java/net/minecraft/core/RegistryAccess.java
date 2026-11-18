package net.minecraft.core;

import java.util.Map;

/** Минимальный NMS {@code RegistryAccess} (Axiom и другие). */
public class RegistryAccess {
    private static final RegistryAccess EMPTY = new RegistryAccess();
    private static final Map<Object, Object> NO_REGISTRIES = Map.of();

    public static RegistryAccess EMPTY_REGISTRY_ACCESS() {
        return EMPTY;
    }

    public static RegistryAccess empty() {
        return EMPTY;
    }

    public Map<Object, Object> registries() {
        return NO_REGISTRIES;
    }

    /** Вложенный {@code RegistryAccess$Frozen}, который ищет Axiom. */
    public static final class Frozen extends RegistryAccess {
        private static final Frozen EMPTY = new Frozen();

        private Frozen() {
        }

        public static Frozen empty() {
            return EMPTY;
        }
    }
}
