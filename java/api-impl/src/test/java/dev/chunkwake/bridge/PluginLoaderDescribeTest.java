package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;

import java.io.File;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Map;
import java.util.jar.JarEntry;
import java.util.jar.JarOutputStream;
import org.bukkit.plugin.PluginDescriptionFile;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

/** Выбор описания: paper-plugin.yml приоритетнее plugin.yml (как на Paper). */
final class PluginLoaderDescribeTest {
    @TempDir
    Path dir;

    private File jar(String name, Map<String, String> entries) throws Exception {
        File file = dir.resolve(name).toFile();
        try (OutputStream out = Files.newOutputStream(file.toPath());
                JarOutputStream jar = new JarOutputStream(out)) {
            for (Map.Entry<String, String> e : entries.entrySet()) {
                jar.putNextEntry(new JarEntry(e.getKey()));
                jar.write(e.getValue().getBytes(StandardCharsets.UTF_8));
                jar.closeEntry();
            }
        }
        return file;
    }

    @Test
    void paperPluginYmlWinsOverPluginYml() throws Exception {
        File file = jar("dual.jar", Map.of(
                "paper-plugin.yml", """
                        name: PaperSide
                        version: 2.0
                        main: xyz.paper.Main
                        api-version: '1.21.8'
                        """,
                "plugin.yml", """
                        name: SpigotSide
                        version: 1.0
                        main: xyz.spigot.Stub
                        api-version: '1.13'
                        """));
        PluginDescriptionFile desc = PluginLoader.describe(file);
        assertEquals("PaperSide", desc.getName());
        assertEquals("xyz.paper.Main", desc.getMain());
        assertEquals("1.21.8", desc.getAPIVersion());
    }

    @Test
    void pluginYmlUsedWhenPaperYmlAbsent() throws Exception {
        File file = jar("spigot.jar", Map.of(
                "plugin.yml", """
                        name: SpigotOnly
                        version: 1.0
                        main: xyz.spigot.Main
                        api-version: '1.20'
                        """));
        PluginDescriptionFile desc = PluginLoader.describe(file);
        assertEquals("SpigotOnly", desc.getName());
        assertEquals("xyz.spigot.Main", desc.getMain());
    }
}
