package dev.chunkwake.bridge;

import org.bukkit.Server;
import org.bukkit.configuration.file.YamlConfiguration;

/** Реальный {@link Server.Spigot}: конфиги не пустые (InvSee++). */
final class SpigotImpl extends Server.Spigot {
    private final YamlConfiguration config = new YamlConfiguration();

    @Override public YamlConfiguration getConfig() { return this.config; }
    @Override public YamlConfiguration getBukkitConfig() { return this.config; }
    @Override public YamlConfiguration getSpigotConfig() { return this.config; }
    @Override public YamlConfiguration getPaperConfig() { return this.config; }
}
