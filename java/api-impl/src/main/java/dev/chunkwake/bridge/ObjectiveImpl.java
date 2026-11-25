package dev.chunkwake.bridge;

import io.papermc.paper.scoreboard.numbers.NumberFormat;
import java.util.Objects;
import net.kyori.adventure.text.Component;
import org.bukkit.OfflinePlayer;
import org.bukkit.entity.Entity;
import org.bukkit.scoreboard.Criteria;
import org.bukkit.scoreboard.DisplaySlot;
import org.bukkit.scoreboard.Objective;
import org.bukkit.scoreboard.RenderType;
import org.bukkit.scoreboard.Score;
import org.bukkit.scoreboard.Scoreboard;

/** {@link Objective}: цель в {@link ScoreboardImpl}. */
final class ObjectiveImpl implements Objective {
    private final ScoreboardImpl scoreboard;
    private final String name;
    private final String criteria;
    private Component displayName;
    private RenderType renderType;
    private boolean autoUpdate = true;
    private NumberFormat numberFormat;
    private volatile boolean removed;

    ObjectiveImpl(ScoreboardImpl scoreboard, String name, String criteria,
            Component displayName, RenderType renderType) {
        this.scoreboard = scoreboard;
        this.name = name;
        this.criteria = criteria;
        this.displayName = displayName;
        this.renderType = renderType == null ? RenderType.INTEGER : renderType;
    }

    void updateFromRust(Component displayName, RenderType renderType) {
        if (this.autoUpdate) {
            this.displayName = displayName;
        }
        this.renderType = renderType;
    }

    void markRemoved() { this.removed = true; }

    boolean isRemoved() { return this.removed; }

    void checkActive() {
        if (this.removed) {
            throw new IllegalStateException("Objective " + this.name + " is not registered");
        }
    }

    @Override public String getName() { return this.name; }

    @Override public Component displayName() { return this.displayName; }

    @Override public void displayName(Component displayName) {
        checkActive();
        this.displayName = Objects.requireNonNull(displayName, "displayName");
        this.scoreboard.request("objective", "display", this.name,
                ScoreboardImpl.json(this.displayName));
    }

    @Override public String getDisplayName() { return ScoreboardImpl.legacy(this.displayName); }

    @Override public void setDisplayName(String displayName) {
        displayName(ChunkwakeServer.legacy(Objects.requireNonNull(displayName, "displayName")));
    }

    @Override public String getCriteria() { return this.criteria; }

    @Override public Criteria getTrackedCriteria() { return CriteriaImpl.of(this.criteria); }

    @Override public boolean isModifiable() { return !getTrackedCriteria().isReadOnly(); }

    @Override public Scoreboard getScoreboard() { return this.scoreboard; }

    @Override public void unregister() { this.scoreboard.unregisterObjective(this); }

    @Override public void setDisplaySlot(DisplaySlot slot) {
        this.scoreboard.setDisplaySlot(this, slot);
    }

    @Override public DisplaySlot getDisplaySlot() {
        return this.scoreboard.displaySlotOf(this.name);
    }

    @Override public void setRenderType(RenderType renderType) {
        checkActive();
        this.renderType = Objects.requireNonNull(renderType, "renderType");
        this.scoreboard.request("objective", "render", this.name,
                renderType == RenderType.HEARTS ? "1" : "0");
    }

    @Override public RenderType getRenderType() { return this.renderType; }

    @Override public Score getScore(OfflinePlayer player) {
        Objects.requireNonNull(player, "player");
        String entry = player.getName();
        if (entry == null) {
            throw new IllegalArgumentException("Player has no name");
        }
        return getScore(entry);
    }

    @Override public Score getScore(String entry) {
        checkActive();
        ScoreboardImpl.checkEntryForApi(entry);
        return new ScoreImpl(this.scoreboard, entry, this);
    }

    @Override public Score getScoreFor(Entity entity) {
        checkActive();
        Objects.requireNonNull(entity, "entity");
        return new ScoreImpl(this.scoreboard, entity.getScoreboardEntryName(), this);
    }

    @Override public boolean willAutoUpdateDisplay() { return this.autoUpdate; }

    @Override public void setAutoUpdateDisplay(boolean autoUpdate) { this.autoUpdate = autoUpdate; }

    @Override public NumberFormat numberFormat() { return this.numberFormat; }

    @Override public void numberFormat(NumberFormat format) {
        checkActive();
        this.numberFormat = format;
        this.scoreboard.setObjectiveNumberFormat(this, format);
    }

    @Override public String toString() { return "Objective{" + this.name + ", criteria=" + this.criteria + "}"; }
}
