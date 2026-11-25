package dev.chunkwake.bridge;

import java.util.Locale;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import org.bukkit.scoreboard.Criteria;
import org.bukkit.scoreboard.RenderType;

/**
 * {@link Criteria}: имя критерия и его ванильные свойства.
 *
 * <p>Rust хранит только способ хранения счёта (`dummy`/`trigger`/прочее) и
 * принимает любое имя критерия, как команда `/scoreboard objectives add`.
 * Java-обёртка сохраняет исходное имя для {@link #getName()} и
 * {@link org.bukkit.scoreboard.Objective#getCriteria()}.
 */
final class CriteriaImpl implements Criteria {
    private static final Set<String> READ_ONLY = Set.of(
            "health", "food", "air", "armor", "xp", "level",
            "deathcount", "playerkillcount", "totalkillcount");

    private static final ConcurrentHashMap<String, CriteriaImpl> CACHE = new ConcurrentHashMap<>();

    private final String name;
    private final boolean readOnly;

    private CriteriaImpl(String name) {
        this.name = name;
        this.readOnly = READ_ONLY.contains(name.toLowerCase(Locale.ROOT))
                || name.toLowerCase(Locale.ROOT).startsWith("teamkill.")
                || name.toLowerCase(Locale.ROOT).startsWith("killedbyteam.");
    }

    /** {@code Bukkit.getScoreboardCriteria(name)}; неизвестные имена тоже принимаются. */
    static CriteriaImpl of(String name) {
        String normalized = name == null ? "" : name.trim();
        if (normalized.isEmpty()) {
            throw new IllegalArgumentException("Criteria name cannot be empty");
        }
        return CACHE.computeIfAbsent(normalized, CriteriaImpl::new);
    }

    @Override public String getName() { return this.name; }

    @Override public boolean isReadOnly() { return this.readOnly; }

    @Override public RenderType getDefaultRenderType() {
        return this.name.equalsIgnoreCase("health") ? RenderType.HEARTS : RenderType.INTEGER;
    }

    @Override public String toString() { return "Criteria{" + this.name + "}"; }
}
