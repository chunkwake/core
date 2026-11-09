package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;

import java.io.File;
import java.io.StringReader;
import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.logging.Logger;
import org.bukkit.plugin.PluginDescriptionFile;
import org.junit.jupiter.api.Test;

class PluginOrderTest {
    private static final Logger LOG = Logger.getLogger("test");

    private static PluginOrder.Candidate plugin(String name, String extra) throws Exception {
        String yaml = "name: " + name + "\nversion: 1\nmain: x." + name + "\n" + extra;
        return new PluginOrder.Candidate(new File(name + ".jar"),
                new PluginDescriptionFile(new StringReader(yaml)));
    }

    private static List<String> names(List<PluginOrder.Candidate> order) {
        List<String> out = new ArrayList<>();
        order.forEach(c -> out.add(c.name()));
        return out;
    }

    @Test
    void dependenciesLoadFirst() throws Exception {
        var order = PluginOrder.sort(List.of(
                plugin("A", "depend: [C]\n"),
                plugin("B", "softdepend: [A, Missing]\n"),
                plugin("C", "")), Set.of(), LOG);
        assertEquals(List.of("C", "A", "B"), names(order));
    }

    @Test
    void loadBeforePutsPluginEarlier() throws Exception {
        var order = PluginOrder.sort(List.of(
                plugin("A", ""),
                plugin("Z", "loadbefore: [A]\n")), Set.of(), LOG);
        assertEquals(List.of("Z", "A"), names(order));
    }

    @Test
    void missingHardDependencyDropsDependents() throws Exception {
        var order = PluginOrder.sort(List.of(
                plugin("A", "depend: [Vault]\n"),
                plugin("B", "depend: [A]\n"),
                plugin("C", "")), Set.of(), LOG);
        assertEquals(List.of("C"), names(order));
    }

    @Test
    void alreadyRegisteredAndProvidedSatisfyDepend() throws Exception {
        var order = PluginOrder.sort(List.of(
                plugin("A", "depend: [ViaVersion, Economy]\n"),
                plugin("Eco", "provides: [Economy]\n")), Set.of("ViaVersion"), LOG);
        assertEquals(List.of("Eco", "A"), names(order));
    }

    @Test
    void softCycleIsBrokenHardCycleIsDropped() throws Exception {
        var soft = PluginOrder.sort(List.of(
                plugin("A", "softdepend: [B]\n"),
                plugin("B", "softdepend: [A]\n")), Set.of(), LOG);
        assertEquals(List.of("A", "B"), names(soft));

        var hard = PluginOrder.sort(List.of(
                plugin("A", "depend: [B]\n"),
                plugin("B", "depend: [A]\n"),
                plugin("C", "")), Set.of(), LOG);
        assertEquals(List.of("C"), names(hard));
    }
}
