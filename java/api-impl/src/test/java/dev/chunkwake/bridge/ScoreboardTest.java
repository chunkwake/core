package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNotSame;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import net.kyori.adventure.text.Component;
import org.bukkit.ChatColor;
import org.bukkit.scoreboard.Criteria;
import org.bukkit.scoreboard.DisplaySlot;
import org.bukkit.scoreboard.Objective;
import org.bukkit.scoreboard.RenderType;
import org.bukkit.scoreboard.Score;
import org.bukkit.scoreboard.Team;
import org.junit.jupiter.api.Test;

/** Буkit scoreboard без native Bridge: локальное состояние и разбор ответов Rust. */
class ScoreboardTest {
    @Test
    void objectivesAndScoresWorkLocally() {
        ScoreboardImpl board = new ScoreboardImpl(null, false);
        Objective kills = board.registerNewObjective("kills", "dummy",
                Component.text("Kills"), RenderType.INTEGER);
        assertSame(kills, board.getObjective("kills"));
        assertEquals("dummy", kills.getCriteria());
        assertEquals("Kills", kills.getDisplayName());
        assertTrue(kills.isModifiable());

        assertTrue(board.getObjectives().contains(kills));
        assertTrue(board.getObjectivesByCriteria("DUMMY").contains(kills));

        Score alice = kills.getScore("Alice");
        assertFalse(alice.isScoreSet());
        assertEquals(0, alice.getScore());
        alice.setScore(3);
        assertTrue(alice.isScoreSet());
        assertEquals(3, kills.getScore("Alice").getScore());
        assertEquals(1, board.getScores("Alice").size());
        assertEquals(1, board.getEntries().size());
        assertTrue(board.getEntries().contains("Alice"));

        alice.resetScore();
        assertFalse(alice.isScoreSet());
        assertThrows(IllegalStateException.class, alice::resetScore);

        assertThrows(IllegalArgumentException.class,
                () -> board.registerNewObjective("kills", "dummy", Component.text("Again"),
                        RenderType.INTEGER));
        assertThrows(IllegalArgumentException.class,
                () -> board.registerNewObjective("this-name-is-too-long", "dummy",
                        Component.text("Long"), RenderType.INTEGER));

        kills.unregister();
        assertNull(board.getObjective("kills"));
        assertThrows(IllegalStateException.class, () -> kills.getScore("Alice"));
    }

    @Test
    void displaySlotsFollowObjective() {
        ScoreboardImpl board = new ScoreboardImpl(null, false);
        Objective kills = board.registerNewObjective("kills", "dummy",
                Component.text("Kills"), RenderType.INTEGER);

        kills.setDisplaySlot(DisplaySlot.SIDEBAR);
        assertSame(kills, board.getObjective(DisplaySlot.SIDEBAR));
        assertEquals(DisplaySlot.SIDEBAR, kills.getDisplaySlot());

        kills.setDisplaySlot(DisplaySlot.BELOW_NAME);
        assertNull(board.getObjective(DisplaySlot.SIDEBAR));
        assertSame(kills, board.getObjective(DisplaySlot.BELOW_NAME));

        board.clearSlot(DisplaySlot.BELOW_NAME);
        assertNull(board.getObjective(DisplaySlot.BELOW_NAME));
        assertNull(kills.getDisplaySlot());

        kills.setDisplaySlot(DisplaySlot.PLAYER_LIST);
        kills.unregister();
        assertNull(board.getObjective(DisplaySlot.PLAYER_LIST));
    }

    @Test
    void teamsAreInMemoryAndMoveEntries() {
        ScoreboardImpl board = new ScoreboardImpl(null, false);
        Team red = board.registerNewTeam("red");
        Team blue = board.registerNewTeam("blue");
        assertSame(red, board.getTeam("red"));
        assertEquals(2, board.getTeams().size());

        red.addEntry("Alice");
        assertTrue(red.hasEntry("Alice"));
        assertEquals(1, red.getSize());
        assertNull(board.getEntryTeam("Bob"));

        blue.addEntry("Alice");
        assertFalse(red.hasEntry("Alice"));
        assertTrue(blue.hasEntry("Alice"));
        assertSame(blue, board.getEntryTeam("Alice"));

        red.addEntry("Bob");
        assertTrue(red.hasEntry("Bob"));
        assertTrue(red.removeEntry("Bob"));
        assertFalse(red.removeEntry("Bob"));

        red.setPrefix("§c[R] ");
        red.setSuffix("§7!");
        red.setDisplayName("§cRed");
        assertEquals("§c[R] ", red.getPrefix());
        assertEquals("§7!", red.getSuffix());
        assertEquals("§cRed", red.getDisplayName());

        red.setColor(ChatColor.RED);
        assertTrue(red.hasColor());
        assertEquals(ChatColor.RED, red.getColor());
        assertEquals(net.kyori.adventure.text.format.NamedTextColor.RED, red.color());

        assertTrue(red.allowFriendlyFire());
        red.setAllowFriendlyFire(false);
        assertFalse(red.allowFriendlyFire());
        assertTrue(red.canSeeFriendlyInvisibles());
        red.setCanSeeFriendlyInvisibles(false);
        assertFalse(red.canSeeFriendlyInvisibles());

        assertEquals(Team.OptionStatus.ALWAYS, red.getOption(Team.Option.NAME_TAG_VISIBILITY));
        red.setNameTagVisibility(org.bukkit.scoreboard.NameTagVisibility.NEVER);
        assertEquals(Team.OptionStatus.NEVER, red.getOption(Team.Option.NAME_TAG_VISIBILITY));
        red.setOption(Team.Option.COLLISION_RULE, Team.OptionStatus.FOR_OWN_TEAM);
        assertEquals(Team.OptionStatus.FOR_OWN_TEAM, red.getOption(Team.Option.COLLISION_RULE));

        assertEquals(0, red.getEntries().size());
        red.unregister();
        assertNull(board.getTeam("red"));
        assertSame(blue, board.getEntryTeam("Alice"));
        assertNotNull(board.registerNewTeam("red"));
        assertThrows(IllegalArgumentException.class,
                () -> board.registerNewTeam("this-name-is-too-long"));
    }

    @Test
    void remoteBoardUsesCompactTabProtocol() {
        FakeBackend rust = new FakeBackend();
        ScoreboardImpl board = new ScoreboardImpl(null, true, rust);

        Objective kills = board.getObjective("kills");
        assertNotNull(kills);
        assertEquals("Kills", kills.getDisplayName());
        assertEquals("dummy", kills.getCriteria());

        Objective deaths = board.registerNewObjective("deaths", "dummy",
                Component.text("Deaths"), RenderType.INTEGER);
        assertNotNull(deaths);

        assertSame(kills, board.getObjective(DisplaySlot.SIDEBAR));
        assertEquals(4, kills.getScore("Alice").getScore());
        assertEquals(1, board.getScores("Alice").size());
        assertEquals(List.of("Alice"), new ArrayList<>(board.getEntries()));

        kills.getScore("Alice").setScore(7);
        kills.getScore("Alice").setTriggerable(false);
        kills.setRenderType(RenderType.HEARTS);
        kills.setDisplayName("Kills!");
        kills.numberFormat(null);
        board.clearSlot(DisplaySlot.SIDEBAR);

        assertTrue(rust.lines.stream()
                .anyMatch(line -> line.startsWith("sb objective\tadd\tdeaths\tdummy\t0\t")),
                rust.lines.toString());
        assertTrue(rust.lines.stream().anyMatch(line -> line.startsWith("sb score\tset\tAlice\tkills\t7")));
        assertTrue(rust.lines.contains("sb display\tclear\tsidebar"));
    }

    @Test
    void scoreboardManagerReturnsStableMainBoard() {
        ChunkwakeServer server = new ChunkwakeServer(Path.of("world"));
        var manager = server.getScoreboardManager();
        assertSame(manager.getMainScoreboard(), manager.getMainScoreboard());
        assertSame(server.mainScoreboard(), manager.getMainScoreboard());
        assertNotSame(manager.getMainScoreboard(), manager.getNewScoreboard());
        assertEquals("dummy", server.getScoreboardCriteria("dummy").getName());
        assertTrue(server.getScoreboardCriteria("health").isReadOnly());
        assertFalse(server.getScoreboardCriteria("dummy").isReadOnly());
        assertEquals("dummy", Criteria.create("dummy").getName());
        assertEquals("dummy", Criteria.DUMMY.getName());
    }

    /** Небольшой Rust-стор: отвечает на те же строки, что PlayPlan::world_request. */
    private static final class FakeBackend implements ScoreboardImpl.Backend {
        final List<String> lines = new ArrayList<>();

        @Override
        public String request(String line) {
            this.lines.add(line);
            if (line.startsWith("sb ")) {
                line = line.substring(3);
            }
            String[] fields = line.split("\t", -1);
            return switch (fields[0]) {
                case "objectives" -> "kills\tdummy\t0\t{\"text\":\"Kills\"}";
                case "display" -> fields.length == 1 || fields[1].equals("list")
                        ? "sidebar\tkills" : "1";
                case "scores" -> "kills\t4";
                case "entries" -> "Alice";
                case "holders" -> "Alice";
                case "score" -> switch (fields[1]) {
                    case "get" -> "4";
                    default -> "1";
                };
                case "trigger" -> "0";
                case "objective" -> "1";
                default -> "";
            };
        }
    }
}
