package dev.chunkwake.bridge;

import org.bukkit.NamespacedKey;
import org.bukkit.boss.BarColor;
import org.bukkit.boss.BarFlag;
import org.bukkit.boss.BarStyle;
import org.bukkit.boss.KeyedBossBar;

/** Минимальная реализация {@link KeyedBossBar}. */
final class KeyedBossBarImpl extends BossBarImpl implements KeyedBossBar {
    private final NamespacedKey key;

    KeyedBossBarImpl(NamespacedKey key, String title, BarColor color, BarStyle style,
            BarFlag[] flags) {
        super(title, color, style, flags);
        this.key = key;
    }

    @Override
    public NamespacedKey getKey() {
        return this.key;
    }
}
