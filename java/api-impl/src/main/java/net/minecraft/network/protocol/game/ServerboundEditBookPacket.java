package net.minecraft.network.protocol.game;

import java.util.List;
import java.util.Optional;
import net.minecraft.network.protocol.Packet;

/** NMS {@code ServerboundEditBookPacket} (CraftEngine). */
public class ServerboundEditBookPacket implements Packet {
    private int slot;
    private List<String> pages = List.of();
    private Optional<String> title = Optional.empty();

    public ServerboundEditBookPacket() {
    }

    public ServerboundEditBookPacket(int slot, List<String> pages, Optional<String> title) {
        this.slot = slot;
        this.pages = pages == null ? List.of() : pages;
        this.title = title == null ? Optional.empty() : title;
    }

    public int getSlot() { return this.slot; }
    public List<String> getPages() { return this.pages; }
    public Optional<String> getTitle() { return this.title; }
}
