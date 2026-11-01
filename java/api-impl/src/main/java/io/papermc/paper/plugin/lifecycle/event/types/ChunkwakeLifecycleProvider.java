package io.papermc.paper.plugin.lifecycle.event.types;

import dev.chunkwake.bridge.Lifecycle;
import io.papermc.paper.plugin.lifecycle.event.LifecycleEvent;
import io.papermc.paper.plugin.lifecycle.event.LifecycleEventOwner;

/**
 * Провайдер типов событий жизненного цикла: Chunkwake для paper-api.
 *
 * Лежит в пакете paper-api вынужденно — сам интерфейс объявлен package-private,
 * а реализовать его извне пакета нельзя. Вся логика — в
 * {@code dev.chunkwake.bridge.Lifecycle}.
 *
 * Находится через {@code ServiceLoader} (см. {@code META-INF/services}); без
 * него падает статическая инициализация {@link LifecycleEvents}, то есть любой
 * плагин, регистрирующий обработчик события жизненного цикла.
 */
public final class ChunkwakeLifecycleProvider implements LifecycleEventTypeProvider {

    @Override
    @SuppressWarnings("unchecked")
    public <O extends LifecycleEventOwner, E extends LifecycleEvent>
            LifecycleEventType.Monitorable<O, E> monitor(String name, Class<? extends O> ownerType) {
        return (LifecycleEventType.Monitorable<O, E>) Lifecycle.monitor(name);
    }

    @Override
    @SuppressWarnings("unchecked")
    public <O extends LifecycleEventOwner, E extends LifecycleEvent>
            LifecycleEventType.Prioritizable<O, E> prioritized(String name, Class<? extends O> ownerType) {
        return (LifecycleEventType.Prioritizable<O, E>) Lifecycle.prioritized(name);
    }

    @Override
    public TagEventTypeProvider tagProvider() {
        return Lifecycle.tags();
    }

    /** Для {@code ServiceLoader}. */
    public ChunkwakeLifecycleProvider() {}
}
