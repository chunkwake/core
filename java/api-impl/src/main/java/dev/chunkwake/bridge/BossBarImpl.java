package dev.chunkwake.bridge;

import java.util.ArrayList;
import java.util.List;
import org.bukkit.boss.BarColor;
import org.bukkit.boss.BarFlag;
import org.bukkit.boss.BarStyle;
import org.bukkit.boss.BossBar;
import org.bukkit.entity.Player;

/** Минимальная реализация {@link BossBar} (EliteMobs и др.). */
class BossBarImpl implements BossBar {
    private String title;
    private BarColor color;
    private BarStyle style;
    private double progress = 1.0D;
    private boolean visible = true;
    private final List<BarFlag> flags = new ArrayList<>();
    private final List<Player> players = new ArrayList<>();

    BossBarImpl(String title, BarColor color, BarStyle style, BarFlag[] flags) {
        this.title = title == null ? "" : title;
        this.color = color == null ? BarColor.WHITE : color;
        this.style = style == null ? BarStyle.SOLID : style;
        if (flags != null) {
            for (BarFlag flag : flags) {
                if (flag != null) {
                    this.flags.add(flag);
                }
            }
        }
    }

    @Override public String getTitle() { return this.title; }
    @Override public void setTitle(String title) { this.title = title == null ? "" : title; }
    @Override public BarColor getColor() { return this.color; }
    @Override public void setColor(BarColor color) { this.color = color; }
    @Override public BarStyle getStyle() { return this.style; }
    @Override public void setStyle(BarStyle style) { this.style = style; }
    @Override public void removeFlag(BarFlag flag) { this.flags.remove(flag); }
    @Override public void addFlag(BarFlag flag) { if (flag != null && !this.flags.contains(flag)) this.flags.add(flag); }
    @Override public boolean hasFlag(BarFlag flag) { return this.flags.contains(flag); }
    @Override public void setProgress(double progress) { this.progress = Math.max(0.0D, Math.min(1.0D, progress)); }
    @Override public double getProgress() { return this.progress; }
    @Override public void addPlayer(Player player) { if (player != null && !this.players.contains(player)) this.players.add(player); }
    @Override public void removePlayer(Player player) { this.players.remove(player); }
    @Override public void removeAll() { this.players.clear(); }
    @Override public List<Player> getPlayers() { return new ArrayList<>(this.players); }
    @Override public void setVisible(boolean visible) { this.visible = visible; }
    @Override public boolean isVisible() { return this.visible; }
    @Override public void show() { this.visible = true; }
    @Override public void hide() { this.visible = false; }
}
