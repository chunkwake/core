package io.papermc.paper.registry.event;

import io.papermc.paper.plugin.bootstrap.BootstrapContext;
import io.papermc.paper.plugin.lifecycle.event.handler.LifecycleEventHandler;
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEventType;
import io.papermc.paper.registry.RegistryBuilder;
import io.papermc.paper.registry.TypedKey;
import io.papermc.paper.registry.event.type.RegistryEntryAddConfiguration;
import io.papermc.paper.registry.event.type.RegistryEntryAddEventType;
import java.util.function.Predicate;

/**
 * Минимальный Paper-совместимый провайдер registry lifecycle events.
 *
 * <p>Paper API загружает реализацию {@link RegistryEventTypeProvider} через
 * ServiceLoader. Без неё {@code RegistryEvents.XXX.compose()} возвращает null
 * и падают bootstrap-плагины (например, HorizonFrontier). События пока не
 * диспатчатся в Rust-реестры, но контракт bootstrap-регистрации существует.
 */
public final class ChunkwakeRegistryEventTypeProvider implements RegistryEventTypeProvider {

    public ChunkwakeRegistryEventTypeProvider() {
    }

    @Override
    public <T, B extends RegistryBuilder<T>> RegistryEntryAddEventType<T, B> registryEntryAdd(
            RegistryEventProvider<T, B> provider) {
        return new RegistryEntryAddEventType<>() {
            @Override
            public String name() {
                return "registry_entry_add";
            }

            @Override
            public RegistryEntryAddConfiguration<T> newHandler(
                    LifecycleEventHandler<? super RegistryEntryAddEvent<T, B>> handler) {
                return new Config<>();
            }
        };
    }

    @Override
    @SuppressWarnings({"unchecked", "rawtypes"})
    public <T, B extends RegistryBuilder<T>>
            LifecycleEventType.Prioritizable<BootstrapContext, RegistryComposeEvent<T, B>> registryCompose(
                    RegistryEventProvider<T, B> provider) {
        return (LifecycleEventType.Prioritizable) dev.chunkwake.bridge.Lifecycle
                .prioritized("registry_compose");
    }

    private static final class Config<T> implements RegistryEntryAddConfiguration<T> {
        @Override
        public RegistryEntryAddConfiguration<T> filter(Predicate<TypedKey<T>> predicate) {
            return this;
        }

        @Override
        public RegistryEntryAddConfiguration<T> priority(int priority) {
            return this;
        }

        @Override
        public RegistryEntryAddConfiguration<T> monitor() {
            return this;
        }
    }
}
