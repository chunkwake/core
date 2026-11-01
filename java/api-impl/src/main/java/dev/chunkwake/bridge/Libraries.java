package dev.chunkwake.bridge;

import io.papermc.paper.plugin.loader.library.LibraryLoadingException;
import io.papermc.paper.plugin.loader.library.impl.MavenLibraryResolver;
import java.io.File;
import java.io.IOException;
import java.net.URL;
import java.util.ArrayList;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Set;
import java.util.logging.Level;
import java.util.logging.Logger;
import org.eclipse.aether.artifact.DefaultArtifact;
import org.eclipse.aether.graph.Dependency;
import org.eclipse.aether.repository.RemoteRepository;

/**
 * Библиотеки, которые плагин объявил в {@code plugin.yml} ({@code libraries:}).
 *
 * Paper скачивает их резолвером из {@code paper-api} в каталог {@code libraries/}
 * и добавляет в classpath плагина. Без этого падают плагины, которые на такие
 * библиотеки рассчитывают: BlueMap объявляет {@code com.flowpowered:flow-math}
 * и без него не стартует.
 *
 * Повторяем это поведение тем же резолвером: он сам качает недостающее,
 * кладёт в локальный репозиторий {@code libraries/} (относительно рабочего
 * каталога JVM) и отдаёт пути к jar'ам. Скачанное остаётся на диске, так что
 * второй запуск в сеть не ходит.
 */
final class Libraries {

    private Libraries() {}

    /**
     * Пути к jar'ам объявленных библиотек. Пустой список — плагин ничего не
     * объявлял. Сеть недоступна — тоже пустой список, но с предупреждением:
     * остальные плагины из-за одного должны загрузиться.
     */
    static List<URL> resolve(File jar, List<String> declared, Logger log) {
        if (declared == null || declared.isEmpty()) {
            return List.of();
        }
        MavenLibraryResolver resolver = new MavenLibraryResolver();
        resolver.addRepository(new RemoteRepository.Builder(
                        "central", "default", MavenLibraryResolver.MAVEN_CENTRAL_DEFAULT_MIRROR)
                .build());
        resolver.addRepository(new RemoteRepository.Builder(
                        "papermc", "default", "https://repo.papermc.io/repository/maven-public/")
                .build());
        for (String coordinate : declared) {
            try {
                resolver.addDependency(new Dependency(new DefaultArtifact(coordinate), "runtime"));
            } catch (IllegalArgumentException e) {
                log.log(Level.WARNING, "plugin " + jar.getName() + ": bad library '" + coordinate + "': "
                        + e.getMessage());
                return List.of();
            }
        }
        Set<URL> urls = new LinkedHashSet<>();
        resolver.register(path -> {
            try {
                urls.add(path.toUri().toURL());
            } catch (IOException e) {
                throw new LibraryLoadingException("library " + path + ": " + e, e);
            }
        });
        List<URL> out = new ArrayList<>(urls);
        log.fine(() -> "plugin " + jar.getName() + ": " + out.size() + " library jar(s) from plugin.yml");
        return out;
    }
}
