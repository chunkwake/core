package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertThrows;

import java.io.File;
import java.io.FileOutputStream;
import java.io.StringReader;
import java.lang.reflect.InvocationTargetException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.concurrent.Callable;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.jar.JarEntry;
import java.util.jar.JarOutputStream;
import java.util.logging.Logger;
import javax.tools.ToolProvider;
import org.bukkit.plugin.PluginDescriptionFile;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

/** Плагин видит классы другого плагина — как в Paper. */
class PluginClassGroupTest {
    /** Открытые загрузчики держат jar: на Windows без закрытия не удалить TempDir. */
    private final List<PluginLoader> open = new java.util.ArrayList<>();

    @org.junit.jupiter.api.AfterEach
    void closeLoaders() throws Exception {
        for (PluginLoader l : open) {
            l.close();
        }
    }

    @TempDir
    Path dir;

    /** Компилирует исходники (имя класса → код) в jar, classpath — jar зависимостей. */
    private File jar(String name, List<String[]> sources, File... classpath) throws Exception {
        Path src = Files.createDirectories(dir.resolve(name + "-src"));
        Path out = Files.createDirectories(dir.resolve(name + "-classes"));
        List<String> args = new java.util.ArrayList<>(List.of("-d", out.toString()));
        if (classpath.length > 0) {
            args.add("-cp");
            args.add(String.join(File.pathSeparator,
                    java.util.Arrays.stream(classpath).map(File::getPath).toList()));
        }
        for (String[] s : sources) {
            Path file = src.resolve(s[0].replace('.', '/') + ".java");
            Files.createDirectories(file.getParent());
            Files.writeString(file, s[1]);
            args.add(file.toString());
        }
        assertEquals(0, ToolProvider.getSystemJavaCompiler().run(null, null, null, args.toArray(String[]::new)));
        File jar = dir.resolve(name + ".jar").toFile();
        try (var j = new JarOutputStream(new FileOutputStream(jar));
                var files = Files.walk(out)) {
            for (Path p : files.filter(Files::isRegularFile).toList()) {
                j.putNextEntry(new JarEntry(out.relativize(p).toString().replace('\\', '/')));
                j.write(Files.readAllBytes(p));
                j.closeEntry();
            }
            j.putNextEntry(new JarEntry("plugin.yml"));
            j.write(("name: " + name + "\nversion: 1\nmain: x.Main\n").getBytes(StandardCharsets.UTF_8));
            j.closeEntry();
        }
        return jar;
    }

    private PluginLoader loader(File jar, String yaml, PluginClassGroup group) throws Exception {
        var desc = new PluginDescriptionFile(new StringReader(yaml));
        var loader = new PluginLoader(null, jar, desc, PluginClassGroupTest.class.getClassLoader(), group);
        open.add(loader);
        return loader;
    }

    private File lib() throws Exception {
        return jar("Lib", List.<String[]>of(new String[] {"lib.Api",
                "package lib; public class Api { public static String hello() { return \"hi\"; } }"}));
    }

    private File app(File lib) throws Exception {
        return jar("App", List.<String[]>of(new String[] {"app.Use",
                "package app; public class Use { public static String call() { return lib.Api.hello(); } }"}), lib);
    }

    @Test
    void pluginSeesClassesOfItsDependency() throws Exception {
        File lib = lib();
        File app = app(lib);
        var group = new PluginClassGroup(Logger.getLogger("test"));
        var libLoader = loader(lib, "name: Lib\nversion: 1\nmain: x.Main\n", group);
        var appLoader = loader(app, "name: App\nversion: 1\nmain: x.Main\ndepend: [Lib]\n", group);
        group.add(libLoader);
        group.add(appLoader);

        Class<?> use = Class.forName("app.Use", true, appLoader);
        assertEquals("hi", use.getMethod("call").invoke(null));
        // Один и тот же класс, а не копия: иначе `instanceof` и статика расходятся.
        assertSame(libLoader.loadClass("lib.Api"), appLoader.loadClass("lib.Api"));
        assertSame(group, appLoader.getGroup());
    }

    @Test
    void closedPluginIsNoLongerVisible() throws Exception {
        File lib = lib();
        File app = app(lib);
        var group = new PluginClassGroup(Logger.getLogger("test"));
        var libLoader = loader(lib, "name: Lib\nversion: 1\nmain: x.Main\n", group);
        var appLoader = loader(app, "name: App\nversion: 1\nmain: x.Main\n", group);
        group.add(libLoader);
        group.add(appLoader);
        libLoader.close();

        Class<?> use = Class.forName("app.Use", true, appLoader);
        var e = assertThrows(InvocationTargetException.class, () -> use.getMethod("call").invoke(null));
        assertEquals(NoClassDefFoundError.class, e.getCause().getClass());
    }

    @Test
    void mutualLookupFromTwoThreadsDoesNotDeadlock() throws Exception {
        // A.v() → B.w(), B.v() → A.w(): каждый загрузчик ищет у другого.
        // Заглушки нужны только компилятору.
        File stubA = jar("StubA", List.<String[]>of(new String[] {"pa.A",
                "package pa; public class A { public static int w() { return 1; } }"}));
        File stubB = jar("StubB", List.<String[]>of(new String[] {"pb.B",
                "package pb; public class B { public static int w() { return 2; } }"}));
        File a = jar("A", List.<String[]>of(new String[] {"pa.A",
                "package pa; public class A { public static int v() { return pb.B.w() + 1; } public static int w() { return 1; } }"}),
                stubB);
        File b = jar("B", List.<String[]>of(new String[] {"pb.B",
                "package pb; public class B { public static int v() { return pa.A.w() + 2; } public static int w() { return 2; } }"}),
                stubA);
        for (int round = 0; round < 20; round++) {
            var group = new PluginClassGroup(Logger.getLogger("test"));
            var la = loader(a, "name: A\nversion: 1\nmain: x.Main\nsoftdepend: [B]\n", group);
            var lb = loader(b, "name: B\nversion: 1\nmain: x.Main\nsoftdepend: [A]\n", group);
            group.add(la);
            group.add(lb);
            var pool = Executors.newFixedThreadPool(2);
            try {
                Callable<Object> ca = () -> Class.forName("pa.A", true, la).getMethod("v").invoke(null);
                Callable<Object> cb = () -> Class.forName("pb.B", true, lb).getMethod("v").invoke(null);
                Future<Object> fa = pool.submit(ca);
                Future<Object> fb = pool.submit(cb);
                assertEquals(3, fa.get(10, java.util.concurrent.TimeUnit.SECONDS));
                assertEquals(3, fb.get(10, java.util.concurrent.TimeUnit.SECONDS));
            } finally {
                pool.shutdownNow();
            }
        }
    }
}
