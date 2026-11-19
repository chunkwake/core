package net.minecraft.network.protocol.game;

import net.minecraft.network.protocol.Packet;

/**
 * NMS {@code ClientboundSectionBlocksUpdatePacket}.
 *
 * <p>Позиции — локальные 12-битные индексы внутри секции, состояния — id
 * блоков. Такая раскладка даёт ProtocolLib `getShortArrays()` /
 * `getIntegerArrays()`.
 */
public class ClientboundSectionBlocksUpdatePacket implements Packet {
    private long sectionPos;
    private short[] positions;
    private int[] states;

    public ClientboundSectionBlocksUpdatePacket() {
    }

    public ClientboundSectionBlocksUpdatePacket(long sectionPos, short[] positions, int[] states) {
        this.sectionPos = sectionPos;
        this.positions = positions == null ? new short[0] : positions;
        this.states = states == null ? new int[0] : states;
    }


    /** Paper/ProtocolLib-конвертеры ищут getter-формы. */
    public long getSectionPos() { return this.sectionPos; }
    public short[] getPositions() { return this.positions; }
    public int[] getStates() { return this.states; }
    public long sectionPos() {
        return this.sectionPos;
    }

    public short[] positions() {
        return this.positions;
    }

    public int[] states() {
        return this.states;
    }
}
