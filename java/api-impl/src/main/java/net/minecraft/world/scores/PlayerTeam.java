package net.minecraft.world.scores;

/** NMS-заглушка {@code PlayerTeam} (CommandAPI проверяет наличие). */
public class PlayerTeam {
    private final net.minecraft.world.scores.Scoreboard scoreboard;
    private final String name;

    public PlayerTeam() {
        this(null, "");
    }

    public PlayerTeam(net.minecraft.world.scores.Scoreboard scoreboard, String name) {
        this.scoreboard = scoreboard;
        this.name = name;
    }
}
