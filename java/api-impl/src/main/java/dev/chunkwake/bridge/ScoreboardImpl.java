package dev.chunkwake.bridge;

import io.papermc.paper.scoreboard.numbers.FixedFormat;
import io.papermc.paper.scoreboard.numbers.NumberFormat;
import io.papermc.paper.scoreboard.numbers.StyledFormat;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.serializer.gson.GsonComponentSerializer;
import net.kyori.adventure.text.serializer.legacy.LegacyComponentSerializer;
import org.bukkit.OfflinePlayer;
import org.bukkit.entity.Entity;
import org.bukkit.scoreboard.Criteria;
import org.bukkit.scoreboard.DisplaySlot;
import org.bukkit.scoreboard.Objective;
import org.bukkit.scoreboard.RenderType;
import org.bukkit.scoreboard.Score;
import org.bukkit.scoreboard.Scoreboard;
import org.bukkit.scoreboard.Team;

/**
 * {@link Scoreboard} поверх состояния Rust.
 *
 * <p>Главный счётчик ({@code remote = true}) держит цели и значения в
 * {@code PlayPlan::scoreboard}: каждая операция уходит строкой {@code sb …}
 * в {@code PlayPlan::world_request}, Rust рассылает ванильные пакеты
 * {@code set_objective}/{@code set_score}/{@code set_display_objective}.
 * Поэтому счёт видят команды {@code /scoreboard} и другие плагины.
 *
 * <p>Счётчики из {@link ScoreboardManager#getNewScoreboard()} живут только в
 * Java: у Rust одно общее состояние, а персональных пакетов для разных
 * scoreboard пока нет. Команды (`{@link Team}) тоже Java-only.
 */
final class ScoreboardImpl implements Scoreboard {
    /** Запрос к Rust; в тестах подменяется фейком без native Bridge. */
    interface Backend {
        String request(String line);

        Backend NATIVE = line -> Bridge.table == null ? "" : Bridge.world(line);
        Backend LOCAL = line -> "";
    }

    private static final GsonComponentSerializer GSON = GsonComponentSerializer.gson();
    private static final LegacyComponentSerializer LEGACY = LegacyComponentSerializer.legacySection();
    private static final int MAX_NAME = 16;

    private final ChunkwakeServer server;
    private final boolean remote;
    private final Backend backend;
    private final Map<String, ObjectiveImpl> objectives = new LinkedHashMap<>();
    private final Map<String, TeamImpl> teams = new LinkedHashMap<>();
    private final Map<String, String> localDisplay = new LinkedHashMap<>();
    private final Map<String, Map<String, Integer>> localScores = new LinkedHashMap<>();
    private final Map<ScoreKey, Component> customNames = new HashMap<>();
    private final Map<ScoreKey, NumberFormat> scoreFormats = new HashMap<>();
    private final Set<ScoreKey> triggers = new HashSet<>();

    ScoreboardImpl(ChunkwakeServer server, boolean remote) {
        this(server, remote, remote ? Backend.NATIVE : Backend.LOCAL);
    }

    /** Конструктор для тестов: фейковый Rust-стор без вызова native Bridge. */
    ScoreboardImpl(ChunkwakeServer server, boolean remote, Backend backend) {
        this.server = server;
        this.remote = remote;
        this.backend = backend;
    }

    boolean isMain() { return this.remote; }

    private record ScoreKey(String entry, String objective) {}

    private static void checkName(String name, String what) {
        if (name == null) {
            throw new NullPointerException(what + " cannot be null");
        }
        if (name.isEmpty()) {
            throw new IllegalArgumentException(what + " cannot be empty");
        }
        if (name.length() > MAX_NAME) {
            throw new IllegalArgumentException(what + " is longer than " + MAX_NAME + " characters");
        }
        checkProtocolSafe(name, what);
    }

    static void checkEntryForApi(String entry) {
        if (entry == null) {
            throw new NullPointerException("entry cannot be null");
        }
        if (entry.isEmpty()) {
            throw new IllegalArgumentException("entry cannot be empty");
        }
        checkProtocolSafe(entry, "entry");
    }

    private static void checkProtocolSafe(String value, String what) {
        if (value.indexOf('\t') >= 0 || value.indexOf('\n') >= 0 || value.indexOf('\r') >= 0) {
            throw new IllegalArgumentException(what + " cannot contain tabs or newlines");
        }
    }

    String request(String... fields) {
        if (!this.remote) {
            return "";
        }
        // `PlayPlan::world_request` разбирает scoreboard-команды по префиксу
        // `sb `; поля внутри — через табуляцию (entry/JSON могут содержать
        // пробелы).
        return this.backend.request("sb " + String.join("\t", fields));
    }

    static String json(Component component) {
        return GSON.serialize(component == null ? Component.empty() : component);
    }

    static Component component(String json, String fallback) {
        if (json == null || json.isBlank()) {
            return Component.text(fallback == null ? "" : fallback);
        }
        try {
            return GSON.deserialize(json);
        } catch (RuntimeException error) {
            return Component.text(fallback == null ? "" : fallback);
        }
    }

    static String legacy(Component component) {
        return LEGACY.serialize(component == null ? Component.empty() : component);
    }

    /** Поля формата числа для Rust: {@code kind} + {@code payload}. */
    static String[] formatFields(NumberFormat format) {
        if (format == null) {
            return new String[] {"", ""};
        }
        if (format instanceof FixedFormat fixed) {
            return new String[] {"fixed", json(fixed.component())};
        }
        if (format instanceof StyledFormat styled) {
            // Протокол ждёт Style; компонент без текста несёт те же поля стиля.
            return new String[] {"styled", json(Component.text().style(styled.style()).build())};
        }
        return new String[] {"blank", ""};
    }

    // --- цели ---

    @Override
    public synchronized Objective registerNewObjective(String name, String criteria,
            Component displayName, RenderType renderType) {
        checkName(name, "Objective name");
        if (criteria == null) {
            throw new IllegalArgumentException("Criteria cannot be null");
        }
        if (criteria.isEmpty()) {
            throw new IllegalArgumentException("Criteria cannot be empty");
        }
        Component display = displayName == null ? Component.text(name) : displayName;
        RenderType render = renderType == null ? RenderType.INTEGER : renderType;
        if (this.objectives.containsKey(name)) {
            throw new IllegalArgumentException("Objective " + name + " already exists");
        }
        if (this.remote) {
            refreshObjectives();
            if (this.objectives.containsKey(name)) {
                throw new IllegalArgumentException("Objective " + name + " already exists");
            }
            String added = request("objective", "add", name, criteria,
                    render == RenderType.HEARTS ? "1" : "0", json(display)).trim();
            if (!"1".equals(added)) {
                throw new IllegalArgumentException("Could not register objective " + name);
            }
        }
        ObjectiveImpl objective = new ObjectiveImpl(this, name, criteria, display, render);
        this.objectives.put(name, objective);
        this.localScores.putIfAbsent(name, new LinkedHashMap<>());
        return objective;
    }

    @Override
    public synchronized Objective registerNewObjective(String name, Criteria criteria,
            Component displayName, RenderType renderType) {
        return registerNewObjective(name, criteria == null ? "dummy" : criteria.getName(),
                displayName, renderType);
    }

    @Override
    public synchronized Objective registerNewObjective(String name, String criteria,
            String displayName, RenderType renderType) {
        Component display = displayName == null ? Component.text(name)
                : ChunkwakeServer.legacy(displayName);
        return registerNewObjective(name, criteria, display, renderType);
    }

    @Override
    public synchronized Objective getObjective(String name) {
        if (name == null) {
            return null;
        }
        if (this.remote) {
            refreshObjectives();
        }
        return this.objectives.get(name);
    }

    ObjectiveImpl objectiveByName(String name, boolean refresh) {
        if (refresh && this.remote) {
            refreshObjectives();
        }
        return this.objectives.get(name);
    }

    @Override
    public synchronized Set<Objective> getObjectivesByCriteria(String criteria) {
        Objects.requireNonNull(criteria, "criteria");
        Set<Objective> out = new LinkedHashSet<>();
        for (Objective objective : getObjectives()) {
            if (objective.getCriteria().equalsIgnoreCase(criteria)) {
                out.add(objective);
            }
        }
        return out;
    }

    @Override
    public synchronized Set<Objective> getObjectivesByCriteria(Criteria criteria) {
        Objects.requireNonNull(criteria, "criteria");
        return getObjectivesByCriteria(criteria.getName());
    }

    @Override
    public synchronized Set<Objective> getObjectives() {
        if (this.remote) {
            refreshObjectives();
        }
        return Collections.unmodifiableSet(new LinkedHashSet<>(this.objectives.values()));
    }

    /** Подтягивает цели, созданные командами и другими плагинами. */
    private synchronized void refreshObjectives() {
        if (!this.remote) {
            return;
        }
        String reply = request("objectives");
        Set<String> seen = new HashSet<>();
        // Rust отдаёт не больше ~7.8 КиБ; если ответ упёрся в предел, об
        // отсутствующих целях судить нельзя — иначе хвост списка «удалится».
        boolean complete = reply.length() < 7600;
        if (!reply.isEmpty()) {
            for (String line : reply.split("\n", -1)) {
                if (line.isEmpty()) {
                    continue;
                }
                String[] fields = line.split("\t", 4);
                if (fields.length < 3) {
                    continue;
                }
                String name = fields[0];
                String criteria = fields[1];
                int render;
                try {
                    render = Integer.parseInt(fields[2]);
                } catch (NumberFormatException error) {
                    continue;
                }
                String displayJson = fields.length > 3 ? fields[3] : "";
                seen.add(name);
                ObjectiveImpl objective = this.objectives.get(name);
                if (objective == null) {
                    objective = new ObjectiveImpl(this, name, criteria,
                            component(displayJson, name),
                            render == 1 ? RenderType.HEARTS : RenderType.INTEGER);
                    this.objectives.put(name, objective);
                    this.localScores.putIfAbsent(name, new LinkedHashMap<>());
                } else {
                    objective.updateFromRust(component(displayJson, name),
                            render == 1 ? RenderType.HEARTS : RenderType.INTEGER);
                }
            }
        }
        if (!complete) {
            return;
        }
        var iterator = this.objectives.entrySet().iterator();
        while (iterator.hasNext()) {
            Map.Entry<String, ObjectiveImpl> entry = iterator.next();
            if (!seen.contains(entry.getKey())) {
                entry.getValue().markRemoved();
                this.localScores.remove(entry.getKey());
                iterator.remove();
            }
        }
    }

    void unregisterObjective(ObjectiveImpl objective) {
        synchronized (this) {
            if (objective.isRemoved()) {
                return;
            }
            if (this.remote) {
                request("objective", "remove", objective.getName());
            }
            this.objectives.remove(objective.getName());
            this.localScores.remove(objective.getName());
            this.localDisplay.values().removeIf(objective.getName()::equals);
            this.customNames.keySet().removeIf(key -> key.objective().equals(objective.getName()));
            this.scoreFormats.keySet().removeIf(key -> key.objective().equals(objective.getName()));
            objective.markRemoved();
        }
    }

    // --- счёт ---

    @Override
    public synchronized Set<Score> getScores(OfflinePlayer player) {
        Objects.requireNonNull(player, "player");
        String name = player.getName();
        return name == null ? Collections.emptySet() : getScores(name);
    }

    @Override
    public synchronized Set<Score> getScores(String entry) {
        checkEntryForApi(entry);
        if (this.remote) {
            refreshObjectives();
        }
        Set<Score> out = new LinkedHashSet<>();
        if (this.remote) {
            for (String line : request("scores", entry).split("\n", -1)) {
                if (line.isEmpty()) {
                    continue;
                }
                String[] fields = line.split("\t", 2);
                ObjectiveImpl objective = this.objectives.get(fields[0]);
                if (objective != null) {
                    out.add(new ScoreImpl(this, entry, objective));
                }
            }
        } else {
            for (Map.Entry<String, Map<String, Integer>> objective : this.localScores.entrySet()) {
                if (objective.getValue().containsKey(entry)) {
                    ObjectiveImpl found = this.objectives.get(objective.getKey());
                    if (found != null) {
                        out.add(new ScoreImpl(this, entry, found));
                    }
                }
            }
        }
        return Collections.unmodifiableSet(out);
    }

    @Override
    public synchronized void resetScores(OfflinePlayer player) {
        Objects.requireNonNull(player, "player");
        String name = player.getName();
        if (name != null) {
            resetScores(name);
        }
    }

    @Override
    public synchronized void resetScores(String entry) {
        checkEntryForApi(entry);
        if (this.remote) {
            request("score", "clear", entry);
        } else {
            for (Map<String, Integer> scores : this.localScores.values()) {
                scores.remove(entry);
            }
        }
        this.customNames.keySet().removeIf(key -> key.entry().equals(entry));
        this.scoreFormats.keySet().removeIf(key -> key.entry().equals(entry));
        this.triggers.removeIf(key -> key.entry().equals(entry));
    }

    /** Значение или {@code null}, если счёта нет. */
    Integer scoreValue(String entry, String objective) {
        if (this.remote) {
            String value = request("score", "get", entry, objective).trim();
            if (value.isEmpty()) {
                return null;
            }
            try {
                return Integer.valueOf(value);
            } catch (NumberFormatException error) {
                return null;
            }
        }
        Map<String, Integer> scores = this.localScores.get(objective);
        return scores == null ? null : scores.get(entry);
    }

    boolean setScoreValue(String entry, String objective, int value) {
        if (this.remote) {
            return "1".equals(request("score", "set", entry, objective,
                    Integer.toString(value)).trim());
        }
        Map<String, Integer> scores = this.localScores.get(objective);
        if (scores == null) {
            return false;
        }
        scores.put(entry, value);
        return true;
    }

    boolean resetScoreValue(String entry, String objective) {
        if (this.remote) {
            return "1".equals(request("score", "reset", entry, objective).trim());
        }
        Map<String, Integer> scores = this.localScores.get(objective);
        if (scores == null) {
            return false;
        }
        boolean removed = scores.remove(entry) != null;
        this.customNames.remove(new ScoreKey(entry, objective));
        this.scoreFormats.remove(new ScoreKey(entry, objective));
        this.triggers.remove(new ScoreKey(entry, objective));
        return removed;
    }

    boolean isTriggerable(String entry, String objective) {
        ScoreKey key = new ScoreKey(entry, objective);
        if (this.remote) {
            return "1".equals(request("trigger", "get", entry, objective).trim());
        }
        return this.triggers.contains(key);
    }

    void setTriggerable(String entry, String objective, boolean triggerable) {
        ScoreKey key = new ScoreKey(entry, objective);
        if (this.remote) {
            request("trigger", "set", entry, objective, triggerable ? "1" : "0");
            return;
        }
        if (triggerable) {
            this.triggers.add(key);
        } else {
            this.triggers.remove(key);
        }
    }

    Component scoreCustomName(String entry, String objective) {
        return this.customNames.get(new ScoreKey(entry, objective));
    }

    void setScoreCustomName(String entry, String objective, Component name) {
        ScoreKey key = new ScoreKey(entry, objective);
        if (name == null) {
            this.customNames.remove(key);
        } else {
            this.customNames.put(key, name);
        }
    }

    NumberFormat scoreNumberFormat(String entry, String objective) {
        return this.scoreFormats.get(new ScoreKey(entry, objective));
    }

    void setScoreNumberFormat(String entry, String objective, NumberFormat format) {
        ScoreKey key = new ScoreKey(entry, objective);
        if (format == null) {
            this.scoreFormats.remove(key);
        } else {
            this.scoreFormats.put(key, format);
        }
        if (this.remote) {
            String[] fields = formatFields(format);
            request("score", "format", entry, objective, fields[0], fields[1]);
        }
    }

    void setObjectiveNumberFormat(ObjectiveImpl objective, NumberFormat format) {
        if (this.remote) {
            String[] fields = formatFields(format);
            request("objective", "format", objective.getName(), fields[0], fields[1]);
        }
    }

    // --- команды ---

    @Override
    public synchronized Team getPlayerTeam(OfflinePlayer player) {
        Objects.requireNonNull(player, "player");
        String name = player.getName();
        return name == null ? null : getEntryTeam(name);
    }

    @Override
    public synchronized Team getEntryTeam(String entry) {
        if (entry == null) {
            return null;
        }
        for (TeamImpl team : this.teams.values()) {
            if (team.hasEntry(entry)) {
                return team;
            }
        }
        return null;
    }

    @Override
    public synchronized Team getTeam(String teamName) {
        return teamName == null ? null : this.teams.get(teamName);
    }

    @Override
    public synchronized Set<Team> getTeams() {
        return Collections.unmodifiableSet(new LinkedHashSet<>(this.teams.values()));
    }

    @Override
    public synchronized Team registerNewTeam(String name) {
        checkName(name, "Team name");
        if (this.teams.containsKey(name)) {
            throw new IllegalArgumentException("Team " + name + " already exists");
        }
        TeamImpl team = new TeamImpl(this, name);
        this.teams.put(name, team);
        return team;
    }

    void unregisterTeam(TeamImpl team) {
        synchronized (this) {
            this.teams.remove(team.getName());
            team.markRemoved();
        }
    }

    void teamChanged(TeamImpl team, String entry, boolean add) {
        synchronized (this) {
            if (add) {
                for (TeamImpl other : this.teams.values()) {
                    if (other != team) {
                        other.removeEntryInternal(entry);
                    }
                }
                team.addEntryInternal(entry);
            } else {
                team.removeEntryInternal(entry);
            }
        }
    }

    // --- общее ---

    @Override
    public synchronized Set<OfflinePlayer> getPlayers() {
        Set<OfflinePlayer> out = new LinkedHashSet<>();
        if (this.server == null) {
            return out;
        }
        for (String entry : getEntries()) {
            if (!entry.isEmpty()) {
                out.add(this.server.getOfflinePlayer(entry));
            }
        }
        return out;
    }

    @Override
    public synchronized Set<String> getEntries() {
        Set<String> out = new LinkedHashSet<>();
        if (this.remote) {
            for (String line : request("entries").split("\n", -1)) {
                if (!line.isEmpty()) {
                    out.add(line);
                }
            }
        } else {
            for (Map<String, Integer> scores : this.localScores.values()) {
                out.addAll(scores.keySet());
            }
        }
        // Команды живут в Java; их записи тоже считаются отслеживаемыми.
        for (TeamImpl team : this.teams.values()) {
            out.addAll(team.getEntries());
        }
        return out;
    }

    @Override
    public synchronized void clearSlot(DisplaySlot slot) {
        Objects.requireNonNull(slot, "slot");
        if (this.remote) {
            request("display", "clear", slot.getId());
        } else {
            this.localDisplay.remove(slot.getId());
        }
    }

    @Override
    public synchronized Set<Score> getScoresFor(Entity entity) {
        Objects.requireNonNull(entity, "entity");
        return getScores(entity.getScoreboardEntryName());
    }

    @Override
    public synchronized void resetScoresFor(Entity entity) {
        Objects.requireNonNull(entity, "entity");
        resetScores(entity.getScoreboardEntryName());
    }

    @Override
    public synchronized Team getEntityTeam(Entity entity) {
        Objects.requireNonNull(entity, "entity");
        return getEntryTeam(entity.getScoreboardEntryName());
    }

    // --- слоты отображения ---

    @Override
    public synchronized Objective getObjective(DisplaySlot slot) {
        Objects.requireNonNull(slot, "slot");
        String name = null;
        if (this.remote) {
            for (String line : request("display", "list").split("\n", -1)) {
                String[] fields = line.split("\t", 2);
                if (fields.length == 2 && fields[0].equals(slot.getId())) {
                    name = fields[1];
                    break;
                }
            }
        } else {
            name = this.localDisplay.get(slot.getId());
        }
        return name == null ? null : getObjective(name);
    }

    /** Слот, в котором сейчас показана цель; {@code null} — ни в каком. */
    DisplaySlot displaySlotOf(String objective) {
        if (this.remote) {
            for (String line : request("display", "list").split("\n", -1)) {
                String[] fields = line.split("\t", 2);
                if (fields.length == 2 && fields[1].equals(objective)) {
                    DisplaySlot slot = displaySlot(fields[0]);
                    if (slot != null) {
                        return slot;
                    }
                }
            }
            return null;
        }
        for (Map.Entry<String, String> entry : this.localDisplay.entrySet()) {
            if (entry.getValue().equals(objective)) {
                return displaySlot(entry.getKey());
            }
        }
        return null;
    }

    void setDisplaySlot(ObjectiveImpl objective, DisplaySlot slot) {
        synchronized (this) {
            if (objective.isRemoved()) {
                throw new IllegalStateException("Objective is not registered");
            }
            DisplaySlot current = displaySlotOf(objective.getName());
            if (current != null && !current.equals(slot)) {
                clearSlot(current);
            }
            if (slot == null) {
                return;
            }
            if (this.remote) {
                String reply = request("display", "set", slot.getId(), objective.getName()).trim();
                if (!"1".equals(reply)) {
                    throw new IllegalArgumentException("Could not display objective " + objective.getName());
                }
            } else {
                this.localDisplay.put(slot.getId(), objective.getName());
            }
        }
    }

    static DisplaySlot displaySlot(String id) {
        for (DisplaySlot slot : DisplaySlot.values()) {
            if (slot.getId().equals(id)) {
                return slot;
            }
        }
        return null;
    }

    // --- утилиты ---

    OfflinePlayer player(String entry) {
        return this.server == null ? null : this.server.getOfflinePlayer(entry);
    }

    ChunkwakeServer server() { return this.server; }

    List<String> entriesOf(String objective) {
        if (this.remote) {
            List<String> out = new ArrayList<>();
            for (String line : request("holders", objective).split("\n", -1)) {
                if (!line.isEmpty()) {
                    out.add(line);
                }
            }
            return out;
        }
        Map<String, Integer> scores = this.localScores.get(objective);
        return scores == null ? List.of() : new ArrayList<>(scores.keySet());
    }
}
