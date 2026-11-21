package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.phys.BlockHitResult;

/** NMS {@code ServerboundUseItemOnPacket} (CraftEngine). */
public class ServerboundUseItemOnPacket implements Packet {
    private BlockHitResult hit;
    private InteractionHand hand;
    private int sequence;
    /** Поле, которое ищет CraftEngine (setTimestamp). */
    private long timestamp;

    public ServerboundUseItemOnPacket() {
    }

    public ServerboundUseItemOnPacket(InteractionHand hand, BlockHitResult hit, int sequence) {
        this.hit = hit;
        this.hand = hand;
        this.sequence = sequence;
    }
}
