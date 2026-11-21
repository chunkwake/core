package net.minecraft.network.protocol.game;

import java.util.Collection;
import java.util.List;
import java.util.Optional;
import net.minecraft.network.protocol.Packet;
import net.minecraft.world.level.saveddata.maps.MapDecoration;
import net.minecraft.world.level.saveddata.maps.MapId;
import net.minecraft.world.level.saveddata.maps.MapItemSavedData;

/** NMS {@code ClientboundMapItemDataPacket} (CraftEngine). */
public class ClientboundMapItemDataPacket implements Packet {
    public ClientboundMapItemDataPacket() {
    }

    public ClientboundMapItemDataPacket(MapId mapId, byte scale, boolean locked,
            Collection<MapDecoration> decorations, MapItemSavedData.MapPatch patch) {
    }

    public ClientboundMapItemDataPacket(MapId mapId, byte scale, boolean locked,
            Optional<List<MapDecoration>> decorations, Optional<MapItemSavedData.MapPatch> patch) {
    }

    public ClientboundMapItemDataPacket(int mapId, byte scale, boolean locked,
            Collection<MapDecoration> decorations, MapItemSavedData.MapPatch patch) {
        this(new MapId(), scale, locked, decorations, patch);
    }
}
