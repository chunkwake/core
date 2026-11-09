package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import io.papermc.paper.plugin.configuration.PluginMeta;
import java.io.StringReader;
import java.util.List;
import java.util.TreeMap;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.TextComponent;
import net.kyori.adventure.text.event.ClickEvent;
import net.kyori.adventure.text.format.NamedTextColor;
import net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.command.defaults.BukkitCommand;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.PluginDescriptionFile;
import org.junit.jupiter.api.Test;

class PaperCommandsTest {
    /** Делегат {@link Plugin} для {@link Stub}: имя, описание и «включён». */
    public static final class FakePlugin {
        private final PluginDescriptionFile desc;
        private final boolean enabled;

        FakePlugin(PluginDescriptionFile desc, boolean enabled) {
            this.desc = desc;
            this.enabled = enabled;
        }

        public String getName() { return desc.getName(); }
        public PluginMeta getPluginMeta() { return desc; }
        public PluginDescriptionFile getDescription() { return desc; }
        public boolean isEnabled() { return enabled; }
    }

    private static Plugin plugin(String yaml, boolean enabled) throws Exception {
        return Stub.of(Plugin.class, new FakePlugin(new PluginDescriptionFile(new StringReader(yaml)), enabled));
    }

    private static String plain(Component c) {
        return PlainTextComponentSerializer.plainText().serialize(c);
    }

    /** Первый текстовый узел с этим содержимым. */
    private static TextComponent find(Component c, String content) {
        if (c instanceof TextComponent t && t.content().equals(content)) {
            return t;
        }
        for (Component child : c.children()) {
            TextComponent hit = find(child, content);
            if (hit != null) {
                return hit;
            }
        }
        return null;
    }

    @Test
    void pluginsListsEnabledGreenAndDisabledRed() throws Exception {
        TreeMap<String, Plugin> bukkit = new TreeMap<>(String.CASE_INSENSITIVE_ORDER);
        bukkit.put("Zeta", plugin("name: Zeta\nversion: 1\nmain: z.Z\napi-version: '1.21'\n", true));
        bukkit.put("alpha", plugin("name: alpha\nversion: 2\nmain: a.A\n", false));
        List<Component> lines = PaperCommands.Plugins.render(new TreeMap<>(), bukkit);
        assertEquals(List.of("ℹ Server Plugins (2):", "Bukkit Plugins:", " - *alpha, Zeta"),
                lines.stream().map(PaperCommandsTest::plain).toList());
        TextComponent zeta = find(lines.get(2), "Zeta");
        assertEquals(NamedTextColor.GREEN, zeta.color());
        assertEquals(ClickEvent.runCommand("/version Zeta"), zeta.clickEvent());
        assertEquals(NamedTextColor.RED, find(lines.get(2), "alpha").color());
    }

    @Test
    void pluginsWrapsEveryTenNamesAndCountsSections() throws Exception {
        TreeMap<String, Plugin> paper = new TreeMap<>(String.CASE_INSENSITIVE_ORDER);
        TreeMap<String, Plugin> bukkit = new TreeMap<>(String.CASE_INSENSITIVE_ORDER);
        for (int i = 0; i < 11; i++) {
            String name = "P" + (char) ('a' + i);
            bukkit.put(name, plugin("name: " + name + "\nversion: 1\nmain: x.X\napi-version: '1.21'\n", true));
        }
        paper.put("Q", plugin("name: Q\nversion: 1\nmain: x.X\napi-version: '1.21'\n", true));
        List<String> lines = PaperCommands.Plugins.render(paper, bukkit).stream().map(PaperCommandsTest::plain).toList();
        assertEquals("ℹ Server Plugins (12):", lines.get(0));
        assertEquals("Paper Plugins (1):", lines.get(1));
        assertEquals(" - Q", lines.get(2));
        assertEquals("Bukkit Plugins (11):", lines.get(3));
        assertTrue(lines.get(4).startsWith(" - Pa, Pb"), lines.get(4));
        assertEquals(" Pk", lines.get(5));
    }

    @Test
    void versionDescribesPlugin() throws Exception {
        Plugin p = plugin("name: Veinminer\nversion: 2.3\nmain: v.V\ndescription: Mines veins\n"
                + "website: https://example.org\nauthors: [A, B, C]\n", true);
        assertEquals("Veinminer version 2.3\nMines veins\nWebsite: https://example.org\nAuthors: A, B, and C",
                plain(PaperCommands.Version.pluginInfo(p)));
    }

    @Test
    void msptColorsByThreshold() {
        List<Component> lines = PaperCommands.Mspt.render("4.25 1 60 45 2 50.5 ? ? ?");
        assertEquals("Server tick times (avg/min/max) from last 5s, 10s, 1m:", plain(lines.get(0)));
        // DecimalFormat без локали, как у Paper: разделитель — системный.
        java.text.DecimalFormat df = new java.text.DecimalFormat("########0.0");
        String a = df.format(4.25), b = df.format(1), c = df.format(60), d = df.format(45), e = df.format(50.5);
        assertEquals("◴ " + a + "/" + b + "/" + c + ", " + d + "/" + df.format(2) + "/" + e + ", ?/?/?",
                plain(lines.get(1)));
        assertEquals(NamedTextColor.GREEN, find(lines.get(1), a).color());
        assertEquals(NamedTextColor.RED, find(lines.get(1), c).color());
        assertEquals(NamedTextColor.YELLOW, find(lines.get(1), d).color());
        assertEquals(NamedTextColor.RED, find(lines.get(1), e).color());
    }

    /** Команда без плагина с алиасом. */
    private static Command command(String name, String... aliases) {
        Command c = new BukkitCommand(name) {
            @Override
            public boolean execute(CommandSender sender, String label, String[] args) {
                return true;
            }
        };
        c.setDescription("Does " + name);
        c.setAliases(List.of(aliases));
        return c;
    }

    @Test
    void helpMapHasCommandsAliasesAndIndex() {
        HelpMapImpl help = new HelpMapImpl();
        help.rebuild(List.of(command("plugins", "pl"), command("version", "ver")));
        assertNotNull(help.getHelpTopic("/plugins"));
        assertEquals("Does plugins", help.getHelpTopic("/plugins").getShortText());
        assertTrue(help.getHelpTopic("/pl").getShortText().contains("Alias for"));
        assertNotNull(help.getHelpTopic("Bukkit"));
        assertNotNull(help.getHelpTopic(""));
        assertNull(help.getHelpTopic("/nothing"));
    }
}
