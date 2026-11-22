package perfprobe;

import com.destroystokyo.paper.event.server.ServerTickEndEvent;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Locale;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.command.PluginCommand;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.server.ServerLoadEvent;
import org.bukkit.plugin.java.JavaPlugin;

/**
 * Собирает MSPT по {@link ServerTickEndEvent}.
 *
 * <p>{@code /perfprobe reset} начинает окно, {@code /perfprobe sample <label>}
 * печатает p50/p95/p99, TPS и пишет сырые samples в
 * {@code plugins/PerfProbe/samples-<label>.csv}, после чего окно очищается.
 */
public final class PerfProbe extends JavaPlugin implements Listener {
    private final List<Double> samples = new ArrayList<>();
    private long windowStartNanos;
    private int windowTicks;

    @Override
    public void onEnable() {
        getDataFolder().mkdirs();
        getServer().getPluginManager().registerEvents(this, this);
        PluginCommand command = getCommand("perfprobe");
        if (command != null) {
            command.setExecutor(this);
        }
        reset();
        getLogger().info("PERFPROBE enabled");
    }

    @Override
    public boolean onCommand(CommandSender sender, Command command, String label, String[] args) {
        if (args.length == 0) {
            sender.sendMessage("usage: /perfprobe reset|sample <label>");
            return true;
        }
        switch (args[0].toLowerCase(Locale.ROOT)) {
            case "reset" -> {
                reset();
                sender.sendMessage("perfprobe: window reset");
            }
            case "sample" -> {
                String name = args.length > 1 ? args[1] : "sample";
                sample(name);
                sender.sendMessage("perfprobe: sampled " + name);
            }
            default -> sender.sendMessage("usage: /perfprobe reset|sample <label>");
        }
        return true;
    }

    @EventHandler
    public void onTick(ServerTickEndEvent event) {
        samples.add(event.getTickDuration());
        windowTicks++;
    }

    @EventHandler
    public void onLoad(ServerLoadEvent event) {
        getLogger().info("PERFPROBE server-load=" + event.getType());
    }

    private void reset() {
        samples.clear();
        windowTicks = 0;
        windowStartNanos = System.nanoTime();
    }

    private void sample(String rawLabel) {
        int count = samples.size();
        if (count == 0) {
            getLogger().info("PERFPROBE SAMPLE label=" + rawLabel + " n=0 (no ticks)");
            return;
        }
        double wall = Math.max(1e-9, (System.nanoTime() - windowStartNanos) / 1_000_000_000.0);
        List<Double> sorted = new ArrayList<>(samples);
        Collections.sort(sorted);
        double avg = 0;
        for (double value : sorted) {
            avg += value;
        }
        avg /= count;
        double p50 = percentile(sorted, 0.50);
        double p95 = percentile(sorted, 0.95);
        double p99 = percentile(sorted, 0.99);
        double max = sorted.get(count - 1);
        double tps = windowTicks / wall;
        double apiTps = getServer().getTPS()[0];
        String label = safe(rawLabel);

        getLogger().info(String.format(Locale.ROOT,
                "PERFPROBE SAMPLE label=%s n=%d ticks=%d wall=%.3f tps=%.2f tps_api=%.2f avg=%.3f p50=%.3f p95=%.3f p99=%.3f max=%.3f",
                label, count, windowTicks, wall, tps, apiTps, avg, p50, p95, p99, max));
        writeCsv(label);
        reset();
    }

    private static double percentile(List<Double> sorted, double fraction) {
        int index = (int) Math.ceil(sorted.size() * fraction) - 1;
        return sorted.get(Math.max(0, Math.min(sorted.size() - 1, index)));
    }

    private void writeCsv(String label) {
        Path path = getDataFolder().toPath().resolve("samples-" + label + ".csv");
        StringBuilder out = new StringBuilder(samples.size() * 16);
        out.append("duration_ms\n");
        for (double value : samples) {
            out.append(String.format(Locale.ROOT, "%.6f%n", value));
        }
        try {
            Files.writeString(path, out.toString(), StandardCharsets.UTF_8);
            getLogger().info("PERFPROBE csv=" + path.toAbsolutePath());
        } catch (IOException error) {
            getLogger().warning("PERFPROBE csv failed: " + error);
        }
    }

    private static String safe(String value) {
        String cleaned = value.replaceAll("[^A-Za-z0-9._-]", "_");
        return cleaned.isEmpty() ? "sample" : cleaned;
    }
}
