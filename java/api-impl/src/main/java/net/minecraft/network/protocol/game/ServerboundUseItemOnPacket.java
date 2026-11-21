package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.phys.BlockHitResult;

/** NMS {@code ServerboundUseItemOnPacket} (CraftEngine). */
public class ServerboundUseItemOnPacket implements Packet {
    private BlockHitResult hit;
    private InteractionHand hand;
    private int sequence;

    public ServerboundUseItemOnPacket() {
    }

    public ServerboundUseItemOnPacket(BlockHitResult hit, InteractionHand hand, int sequence) {
        this.hit = hit;
        this.hand = hand;
        this.sequence = sequence;
    }
}
