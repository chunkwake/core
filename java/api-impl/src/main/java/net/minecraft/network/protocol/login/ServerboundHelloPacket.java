package net.minecraft.network.protocol.login;

import java.util.Optional;
import java.util.UUID;
import net.minecraft.network.protocol.Packet;

/** NMS {@code ServerboundHelloPacket} (CraftEngine). */
public class ServerboundHelloPacket implements Packet {
    private String name = "";
    private UUID profileId;
    private Optional<UUID> profileIdLegacy = Optional.empty();

    public ServerboundHelloPacket() {
    }

    public ServerboundHelloPacket(String name, UUID profileId) {
        this.name = name;
        this.profileId = profileId;
    }

    public ServerboundHelloPacket(String name, Optional<UUID> profileId) {
        this.name = name;
        this.profileIdLegacy = profileId == null ? Optional.empty() : profileId;
        this.profileId = this.profileIdLegacy.orElse(null);
    }

    public String getName() { return this.name; }
    public UUID getProfileId() { return this.profileId; }
    public Optional<UUID> getProfileId$legacy() { return this.profileIdLegacy; }
}
