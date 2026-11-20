package net.minecraft.core;

import java.util.Map;

/**
 * NMS {@code RegistryAccess} — интерфейс (как в ванили): squaremap и другие
 * плагины проверяют его как интерфейс, класс даёт
 * {@code IncompatibleClassChangeError}.
 */
public interface RegistryAccess {
    RegistryAccess EMPTY = new RegistryAccess() {
    };

    static RegistryAccess EMPTY_REGISTRY_ACCESS() {
        return EMPTY;
    }

    static RegistryAccess empty() {
        return EMPTY;
    }

    default Map<Object, Object> registries() {
        return Map.of();
    }

    /** Заглушка {@code RegistryAccess$Frozen}. */
    final class Frozen implements RegistryAccess {
        private static final Frozen EMPTY = new Frozen();

        private Frozen() {
        }

        public static Frozen empty() {
            return EMPTY;
        }
    }
}
