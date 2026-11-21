package net.minecraft.network.protocol.game;

import java.util.Collection;
import java.util.List;
import java.util.Map;
import java.util.Set;
import net.minecraft.advancements.AdvancementHolder;
import net.minecraft.advancements.AdvancementProgress;
import net.minecraft.network.protocol.Packet;
import net.minecraft.resources.Identifier;

/** NMS {@code ClientboundUpdateAdvancementsPacket} (CraftEngine). */
public class ClientboundUpdateAdvancementsPacket implements Packet {
    public ClientboundUpdateAdvancementsPacket() {
    }

    public ClientboundUpdateAdvancementsPacket(boolean reset,
            List<AdvancementHolder> added, Set<Identifier> removed,
            Map<Identifier, AdvancementProgress> progress, boolean showAdvancements) {
    }

    public ClientboundUpdateAdvancementsPacket(boolean reset,
            Collection<AdvancementHolder> added, Set<Identifier> removed,
            Map<Identifier, AdvancementProgress> progress) {
    }
}
