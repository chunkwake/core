package dev.chunkwake.bridge;

import io.papermc.paper.scoreboard.numbers.NumberFormat;
import net.kyori.adventure.text.Component;
import org.bukkit.OfflinePlayer;
import org.bukkit.scoreboard.Objective;
import org.bukkit.scoreboard.Score;
import org.bukkit.scoreboard.Scoreboard;

/** {@link Score}: значение держателя в цели. */
final class ScoreImpl implements Score {
    private final ScoreboardImpl scoreboard;
    private final String entry;
    private final ObjectiveImpl objective;

    ScoreImpl(ScoreboardImpl scoreboard, String entry, ObjectiveImpl objective) {
        this.scoreboard = scoreboard;
        this.entry = entry;
        this.objective = objective;
    }

    @Override public OfflinePlayer getPlayer() { return this.scoreboard.player(this.entry); }

    @Override public String getEntry() { return this.entry; }

    @Override public Objective getObjective() { return this.objective; }

    @Override public int getScore() {
        Integer value = this.scoreboard.scoreValue(this.entry, this.objective.getName());
        return value == null ? 0 : value;
    }

    @Override public void setScore(int score) {
        this.objective.checkActive();
        if (!this.scoreboard.setScoreValue(this.entry, this.objective.getName(), score)) {
            throw new IllegalStateException("Score " + this.entry + "/" + this.objective.getName()
                    + " is not accepted by the server");
        }
    }

    @Override public boolean isScoreSet() {
        return this.scoreboard.scoreValue(this.entry, this.objective.getName()) != null;
    }

    @Override public Scoreboard getScoreboard() { return this.scoreboard; }

    @Override public void resetScore() {
        if (!isScoreSet()) {
            throw new IllegalStateException("Score is not set");
        }
        this.scoreboard.resetScoreValue(this.entry, this.objective.getName());
    }

    @Override public boolean isTriggerable() {
        return this.scoreboard.isTriggerable(this.entry, this.objective.getName());
    }

    @Override public void setTriggerable(boolean triggerable) {
        this.objective.checkActive();
        this.scoreboard.setTriggerable(this.entry, this.objective.getName(), triggerable);
    }

    @Override public Component customName() {
        return this.scoreboard.scoreCustomName(this.entry, this.objective.getName());
    }

    @Override public void customName(Component name) {
        this.scoreboard.setScoreCustomName(this.entry, this.objective.getName(), name);
    }

    @Override public NumberFormat numberFormat() {
        return this.scoreboard.scoreNumberFormat(this.entry, this.objective.getName());
    }

    @Override public void numberFormat(NumberFormat format) {
        this.scoreboard.setScoreNumberFormat(this.entry, this.objective.getName(), format);
    }

    @Override public String toString() {
        return "Score{" + this.entry + ", " + this.objective.getName() + "=" + getScore() + "}";
    }
}
