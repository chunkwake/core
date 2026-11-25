package dev.chunkwake.bridge;

import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.EnumMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import net.kyori.adventure.audience.Audience;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.format.NamedTextColor;
import net.kyori.adventure.text.format.TextColor;
import org.bukkit.ChatColor;
import org.bukkit.OfflinePlayer;
import org.bukkit.entity.Entity;
import org.bukkit.entity.Player;
import org.bukkit.scoreboard.NameTagVisibility;
import org.bukkit.scoreboard.Scoreboard;
import org.bukkit.scoreboard.Team;

/**
 * {@link Team}: Java-состояние команды.
 *
 * <p>Rust уже хранит команды для дружественного огня (`/team`), но пакетов
 * {@code set_player_team} в протоколе Rust нет. Поэтому здесь команда живёт
 * только в Java и клиенту не отправляется: префиксы/цвета не видны в игре,
 * а членство не влияет на урон Rust. Это честно отмечено в фич-матрице.
 */
final class TeamImpl implements Team {
    private final ScoreboardImpl scoreboard;
    private final String name;
    private final Set<String> entries = new LinkedHashSet<>();
    private final Map<Option, OptionStatus> options = new EnumMap<>(Option.class);
    private Component displayName;
    private Component prefix = Component.empty();
    private Component suffix = Component.empty();
    private ChatColor chatColor;
    private TextColor textColor;
    private boolean friendlyFire = true;
    private boolean seeFriendlyInvisibles = true;
    private volatile boolean removed;

    TeamImpl(ScoreboardImpl scoreboard, String name) {
        this.scoreboard = scoreboard;
        this.name = name;
        this.displayName = Component.text(name);
        for (Option option : Option.values()) {
            this.options.put(option, OptionStatus.ALWAYS);
        }
    }

    void markRemoved() { this.removed = true; }

    void checkActive() {
        if (this.removed) {
            throw new IllegalStateException("Team " + this.name + " is not registered");
        }
    }

    @Override public String getName() { return this.name; }

    @Override public Component displayName() { return this.displayName; }

    @Override public void displayName(Component displayName) {
        checkActive();
        this.displayName = Objects.requireNonNull(displayName, "displayName");
    }

    @Override public Component prefix() { return this.prefix; }

    @Override public void prefix(Component prefix) {
        checkActive();
        this.prefix = Objects.requireNonNull(prefix, "prefix");
    }

    @Override public Component suffix() { return this.suffix; }

    @Override public void suffix(Component suffix) {
        checkActive();
        this.suffix = Objects.requireNonNull(suffix, "suffix");
    }

    @Override public boolean hasColor() { return this.textColor != null || this.chatColor != null; }

    @Override public TextColor color() { return this.textColor; }

    @Override public void color(NamedTextColor color) {
        checkActive();
        this.textColor = color;
        this.chatColor = color == null ? null : chatColor(color);
    }

    private static ChatColor chatColor(NamedTextColor color) {
        try {
            return ChatColor.valueOf(color.toString().toUpperCase(Locale.ROOT));
        } catch (IllegalArgumentException error) {
            return null;
        }
    }

    @Override public String getDisplayName() { return ScoreboardImpl.legacy(this.displayName); }

    @Override public void setDisplayName(String displayName) {
        displayName(ChunkwakeServer.legacy(Objects.requireNonNull(displayName, "displayName")));
    }

    @Override public String getPrefix() { return ScoreboardImpl.legacy(this.prefix); }

    @Override public void setPrefix(String prefix) {
        prefix(ChunkwakeServer.legacy(Objects.requireNonNull(prefix, "prefix")));
    }

    @Override public String getSuffix() { return ScoreboardImpl.legacy(this.suffix); }

    @Override public void setSuffix(String suffix) {
        suffix(ChunkwakeServer.legacy(Objects.requireNonNull(suffix, "suffix")));
    }

    @Override public ChatColor getColor() {
        return this.chatColor == null ? ChatColor.RESET : this.chatColor;
    }

    @Override public void setColor(ChatColor color) {
        checkActive();
        this.chatColor = color;
        if (color == null || color == ChatColor.RESET) {
            this.textColor = null;
            return;
        }
        NamedTextColor named = NamedTextColor.NAMES.value(color.name().toLowerCase(Locale.ROOT));
        this.textColor = named;
    }

    @Override public boolean allowFriendlyFire() { return this.friendlyFire; }

    @Override public void setAllowFriendlyFire(boolean friendlyFire) {
        checkActive();
        this.friendlyFire = friendlyFire;
    }

    @Override public boolean canSeeFriendlyInvisibles() { return this.seeFriendlyInvisibles; }

    @Override public void setCanSeeFriendlyInvisibles(boolean visible) {
        checkActive();
        this.seeFriendlyInvisibles = visible;
    }

    @Override public NameTagVisibility getNameTagVisibility() {
        return switch (getOption(Option.NAME_TAG_VISIBILITY)) {
            case ALWAYS -> NameTagVisibility.ALWAYS;
            case NEVER -> NameTagVisibility.NEVER;
            case FOR_OTHER_TEAMS -> NameTagVisibility.HIDE_FOR_OTHER_TEAMS;
            case FOR_OWN_TEAM -> NameTagVisibility.HIDE_FOR_OWN_TEAM;
        };
    }

    @Override public void setNameTagVisibility(NameTagVisibility visibility) {
        setOption(Option.NAME_TAG_VISIBILITY, optionStatus(visibility));
    }

    @Override public Set<OfflinePlayer> getPlayers() {
        Set<OfflinePlayer> out = new LinkedHashSet<>();
        if (this.scoreboard.server() != null) {
            for (String entry : this.entries) {
                out.add(this.scoreboard.server().getOfflinePlayer(entry));
            }
        }
        return Collections.unmodifiableSet(out);
    }

    @Override public Set<String> getEntries() {
        return Collections.unmodifiableSet(new LinkedHashSet<>(this.entries));
    }

    @Override public int getSize() { return this.entries.size(); }

    @Override public Scoreboard getScoreboard() { return this.scoreboard; }

    @Override public void addPlayer(OfflinePlayer player) {
        Objects.requireNonNull(player, "player");
        String name = player.getName();
        if (name == null) {
            throw new IllegalArgumentException("Player has no name");
        }
        addEntry(name);
    }

    @Override public void addEntry(String entry) {
        checkActive();
        ScoreboardImpl.checkEntryForApi(entry);
        this.scoreboard.teamChanged(this, entry, true);
    }

    @Override public void addEntities(Collection<Entity> entities) {
        Objects.requireNonNull(entities, "entities");
        for (Entity entity : entities) {
            addEntity(entity);
        }
    }

    @Override public void addEntity(Entity entity) {
        Objects.requireNonNull(entity, "entity");
        addEntry(entity.getScoreboardEntryName());
    }

    @Override public void addEntries(Collection<String> entries) {
        Objects.requireNonNull(entries, "entries");
        for (String entry : entries) {
            addEntry(entry);
        }
    }

    @Override public boolean removePlayer(OfflinePlayer player) {
        Objects.requireNonNull(player, "player");
        String name = player.getName();
        return name != null && removeEntry(name);
    }

    @Override public boolean removeEntry(String entry) {
        if (entry == null) {
            return false;
        }
        if (!this.entries.contains(entry)) {
            return false;
        }
        this.scoreboard.teamChanged(this, entry, false);
        return true;
    }

    @Override public boolean removeEntities(Collection<Entity> entities) {
        Objects.requireNonNull(entities, "entities");
        boolean changed = false;
        for (Entity entity : entities) {
            changed |= removeEntity(entity);
        }
        return changed;
    }

    @Override public boolean removeEntity(Entity entity) {
        Objects.requireNonNull(entity, "entity");
        return removeEntry(entity.getScoreboardEntryName());
    }

    @Override public boolean removeEntries(Collection<String> entries) {
        Objects.requireNonNull(entries, "entries");
        boolean changed = false;
        for (String entry : entries) {
            changed |= removeEntry(entry);
        }
        return changed;
    }

    @Override public void unregister() { this.scoreboard.unregisterTeam(this); }

    @Override public boolean hasPlayer(OfflinePlayer player) {
        Objects.requireNonNull(player, "player");
        String name = player.getName();
        return name != null && hasEntry(name);
    }

    @Override public boolean hasEntry(String entry) {
        return entry != null && this.entries.contains(entry);
    }

    @Override public boolean hasEntity(Entity entity) {
        Objects.requireNonNull(entity, "entity");
        return hasEntry(entity.getScoreboardEntryName());
    }

    @Override public OptionStatus getOption(Option option) {
        Objects.requireNonNull(option, "option");
        return this.options.getOrDefault(option, OptionStatus.ALWAYS);
    }

    @Override public void setOption(Option option, OptionStatus status) {
        checkActive();
        Objects.requireNonNull(option, "option");
        Objects.requireNonNull(status, "status");
        this.options.put(option, status);
    }

    private static OptionStatus optionStatus(NameTagVisibility visibility) {
        return switch (visibility) {
            case ALWAYS -> OptionStatus.ALWAYS;
            case NEVER -> OptionStatus.NEVER;
            case HIDE_FOR_OTHER_TEAMS -> OptionStatus.FOR_OTHER_TEAMS;
            case HIDE_FOR_OWN_TEAM -> OptionStatus.FOR_OWN_TEAM;
        };
    }

    @Override public Iterable<? extends Audience> audiences() {
        List<Audience> out = new ArrayList<>();
        if (this.scoreboard.server() != null) {
            for (Player player : this.scoreboard.server().getOnlinePlayers()) {
                if (this.entries.contains(player.getName())) {
                    out.add(player);
                }
            }
        }
        return out;
    }

    // --- вызовы из ScoreboardImpl ---

    void addEntryInternal(String entry) {
        this.entries.add(entry);
    }

    boolean removeEntryInternal(String entry) {
        return this.entries.remove(entry);
    }

    @Override public String toString() { return "Team{" + this.name + ", entries=" + this.entries.size() + "}"; }
}
