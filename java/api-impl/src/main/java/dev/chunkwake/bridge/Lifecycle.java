package dev.chunkwake.bridge;

import io.papermc.paper.plugin.bootstrap.BootstrapContext;
import io.papermc.paper.plugin.lifecycle.event.LifecycleEvent;
import io.papermc.paper.plugin.lifecycle.event.LifecycleEventManager;
import io.papermc.paper.plugin.lifecycle.event.LifecycleEventOwner;
import io.papermc.paper.plugin.lifecycle.event.handler.LifecycleEventHandler;
import io.papermc.paper.plugin.lifecycle.event.handler.configuration.LifecycleEventHandlerConfiguration;
import io.papermc.paper.plugin.lifecycle.event.handler.configuration.MonitorLifecycleEventHandlerConfiguration;
import io.papermc.paper.plugin.lifecycle.event.handler.configuration.PrioritizedLifecycleEventHandlerConfiguration;
import io.papermc.paper.plugin.lifecycle.event.registrar.ReloadableRegistrarEvent;
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEventType;
import io.papermc.paper.plugin.lifecycle.event.types.TagEventTypeProvider;
import io.papermc.paper.registry.RegistryKey;
import io.papermc.paper.tag.PostFlattenTagRegistrar;
import io.papermc.paper.tag.PreFlattenTagRegistrar;
import java.util.function.Supplier;

/**
 * События жизненного цикла плагинов (paper-api).
 *
 * Провайдер типов ({@code LifecycleEventTypeProvider}) в paper-api объявлен
 * package-private и ищется через {@code ServiceLoader}, поэтому его реализация
 * живёт в пакете paper-api ({@code io.papermc.paper.plugin.lifecycle.event.types})
 * и только делегирует сюда: здесь типы собираются без ограничений пакета.
 *
 * Диспетчеризации у нас нет — регистрация обработчика запоминается и проходит
 * без ошибки, но сам обработчик не вызывается: события бумажного жизненного
 * цикла (команды через {@code LifecycleEvents.COMMANDS}, теги) в Chunkwake не
 * наступают. Команды плагинов регистрируются обычным путём через
 * {@code plugin.yml}.
 */
public final class Lifecycle {

    private Lifecycle() {}

    /** Тип события, у которого запоминается имя (для диагностики). */
    interface Named {
        String name();
    }

    /** Событие с монитором: {@code newHandler} отдаёт конфигурацию монитора. */
    static final class Monitor<O extends LifecycleEventOwner, E extends LifecycleEvent>
            implements LifecycleEventType.Monitorable<O, E>, Named {

        private final String name;

        Monitor(String name) {
            this.name = name;
        }

        @Override
        public String name() {
            return name;
        }

        @Override
        public MonitorLifecycleEventHandlerConfiguration<O> newHandler(LifecycleEventHandler<? super E> handler) {
            return new MonitorConfig<>(name, handler);
        }

        @Override
        public String toString() {
            return "LifecycleEventType[" + name + "]";
        }
    }

    /** Событие с приоритетом: {@code newHandler} отдаёт конфигурацию приоритета. */
    static final class Prioritized<O extends LifecycleEventOwner, E extends LifecycleEvent>
            implements LifecycleEventType.Prioritizable<O, E>, Named {

        private final String name;

        Prioritized(String name) {
            this.name = name;
        }

        @Override
        public String name() {
            return name;
        }

        @Override
        public PrioritizedLifecycleEventHandlerConfiguration<O> newHandler(
                LifecycleEventHandler<? super E> handler) {
            return new PriorityConfig<>(name, handler);
        }

        @Override
        public String toString() {
            return "LifecycleEventType[" + name + "]";
        }
    }

    static final class MonitorConfig<O extends LifecycleEventOwner>
            implements MonitorLifecycleEventHandlerConfiguration<O>, Named {

        private final String name;
        private final LifecycleEventHandler<?> handler;

        MonitorConfig(String name, LifecycleEventHandler<?> handler) {
            this.name = name;
            this.handler = handler;
        }

        @Override
        public String name() {
            return name;
        }

        /** Обработчик, который плагин передал в {@code newHandler}. */
        LifecycleEventHandler<?> handler() {
            return handler;
        }

        @Override
        public MonitorLifecycleEventHandlerConfiguration<O> monitor() {
            return this;
        }

        @Override
        public String toString() {
            return "monitor[" + name + "]";
        }
    }

    static final class PriorityConfig<O extends LifecycleEventOwner>
            implements PrioritizedLifecycleEventHandlerConfiguration<O>, Named {

        private final String name;
        private final LifecycleEventHandler<?> handler;
        private int priority;
        private boolean monitor;

        PriorityConfig(String name, LifecycleEventHandler<?> handler) {
            this.name = name;
            this.handler = handler;
        }

        @Override
        public String name() {
            return name;
        }

        /** Обработчик, который плагин передал в {@code newHandler}. */
        LifecycleEventHandler<?> handler() {
            return handler;
        }

        @Override
        public PrioritizedLifecycleEventHandlerConfiguration<O> priority(int priority) {
            this.priority = priority;
            return this;
        }

        @Override
        public PrioritizedLifecycleEventHandlerConfiguration<O> monitor() {
            monitor = true;
            return this;
        }

        @Override
        public String toString() {
            return "prioritized[" + name + ", priority " + priority + (monitor ? ", monitor" : "") + "]";
        }
    }

    /** Теговые события: тип на каждый вызов, вызовов не бывает. */
    static final class Tags implements TagEventTypeProvider {

        @Override
        public <T> LifecycleEventType.Prioritizable<BootstrapContext, ReloadableRegistrarEvent<PreFlattenTagRegistrar<T>>>
                preFlatten(RegistryKey<T> key) {
            return new Prioritized<>("pre_flatten");
        }

        @Override
        public <T> LifecycleEventType.Prioritizable<BootstrapContext, ReloadableRegistrarEvent<PostFlattenTagRegistrar<T>>>
                postFlatten(RegistryKey<T> key) {
            return new Prioritized<>("post_flatten");
        }
    }

    public static <O extends LifecycleEventOwner, E extends LifecycleEvent> LifecycleEventType.Monitorable<O, E>
            monitor(String name) {
        return new Monitor<>(name);
    }

    public static <O extends LifecycleEventOwner, E extends LifecycleEvent> LifecycleEventType.Prioritizable<O, E>
            prioritized(String name) {
        return new Prioritized<>(name);
    }

    public static TagEventTypeProvider tags() {
        return new Tags();
    }

    /**
     * Событие {@code LifecycleEvents.COMMANDS} наступает сразу при регистрации:
     * у нас нет отдельной фазы «загрузка команд», а плагин, который зовёт
     * {@code getLifecycleManager().registerEventHandler(LifecycleEvents.COMMANDS, ...)},
     * ожидает, что его команды появятся в игре.
     */
    static void dispatch(Manager<?> manager, LifecycleEventHandlerConfiguration<?> configuration) {
        ChunkwakeServer server = Bridge.server;
        if (server == null) {
            return;
        }
        LifecycleEventHandler<?> handler = switch (configuration) {
            case MonitorConfig<?> c -> c.handler();
            case PriorityConfig<?> c -> c.handler();
            default -> null;
        };
        if (handler == null) {
            return;
        }
        server.runLifecycleCommands(manager.ownerName(), configuration, handler);
    }

    /** Менеджер событий жизненного цикла плагина: регистрация проходит, вызовов нет. */
    public static <O extends LifecycleEventOwner> LifecycleEventManager<O> manager(String owner) {
        return new Manager<>(() -> owner);
    }

    /**
     * Имя владельца на момент регистрации: конструктор {@link org.bukkit.plugin.java.JavaPlugin}
     * запрашивает менеджер раньше, чем получает метаданные.
     */
    public static <O extends LifecycleEventOwner> LifecycleEventManager<O> manager(Supplier<String> owner) {
        return new Manager<>(owner);
    }

    private static final class Manager<O extends LifecycleEventOwner> implements LifecycleEventManager<O> {

        private final Supplier<String> owner;

        Manager(Supplier<String> owner) {
            this.owner = owner;
        }

        String ownerName() {
            return owner.get();
        }

    @Override
    public void registerEventHandler(LifecycleEventHandlerConfiguration<? super O> configuration) {
        Bridge.log(3, "lifecycle[" + owner.get() + "]: registered " + configuration);
        Lifecycle.dispatch(this, configuration);
    }

        @Override
        public String toString() {
            return "LifecycleEventManager[" + owner.get() + "]";
        }
    }
}
