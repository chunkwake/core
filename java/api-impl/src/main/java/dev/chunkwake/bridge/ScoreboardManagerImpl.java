package dev.chunkwake.bridge;

import org.bukkit.scoreboard.Scoreboard;
import org.bukkit.scoreboard.ScoreboardManager;

/**
 * {@link ScoreboardManager} сервера.
 *
 * <p>Главный счётчик — тот, что живёт в Rust: цели, значения и слоты видны
 * командам и всем плагинам. {@link #getNewScoreboard()} создаёт отдельный
 * Java-счётчик; он пока не отправляется клиенту (см. docs/feature-matrix.md).
 */
final class ScoreboardManagerImpl {
    private final ChunkwakeServer server;
    private final ScoreboardImpl main;
    final ScoreboardManager api = Stub.of(ScoreboardManager.class, this);

    ScoreboardManagerImpl(ChunkwakeServer server) {
        this.server = server;
        this.main = new ScoreboardImpl(server, true);
    }

    ScoreboardImpl mainImpl() { return this.main; }

    public Scoreboard getMainScoreboard() { return this.main; }

    public Scoreboard getNewScoreboard() { return new ScoreboardImpl(this.server, false); }
}
