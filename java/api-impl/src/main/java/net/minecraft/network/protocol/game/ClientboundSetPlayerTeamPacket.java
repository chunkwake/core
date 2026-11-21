package net.minecraft.network.protocol.game;

/**
 * Заглушка NMS {@code ClientboundSetPlayerTeamPacket}: нужна для загрузки классов плагинов
 * (NpcPlugin и подобные ссылаются на пакеты в своих методах). Поведение
 * пакета в Chunkwake не используется — пакеты собираются в Rust.
 */
public class ClientboundSetPlayerTeamPacket {
    private String name = "";
    private int method;
    private java.util.Optional<net.minecraft.network.chat.Component> displayName =
            java.util.Optional.empty();
    private java.util.Collection<String> players = java.util.List.of();

    public ClientboundSetPlayerTeamPacket() {
    }

    public ClientboundSetPlayerTeamPacket(String name, int method,
            java.util.Optional<net.minecraft.network.chat.Component> displayName,
            java.util.Collection<String> players) {
        this.name = name;
        this.method = method;
        this.displayName = displayName == null ? java.util.Optional.empty() : displayName;
        this.players = players == null ? java.util.List.of() : players;
    }

    public static ClientboundSetPlayerTeamPacket createAddOrModifyPacket(
            net.minecraft.world.scores.PlayerTeam team, boolean update) {
        return new ClientboundSetPlayerTeamPacket();
    }
}
