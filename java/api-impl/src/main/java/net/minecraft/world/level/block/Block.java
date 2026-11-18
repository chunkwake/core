package net.minecraft.world.level.block;

/**
 * Базовая NMS-реализация блока. У каждого блока есть стабильный default
 * block state (vanilla хранит его полем) — на идентичности состояний
 * держатся палитры чанков и реестр Axiom.
 */
public class Block {
    public static final net.minecraft.core.IdMapper<net.minecraft.world.level.block.state.BlockState>
            BLOCK_STATE_REGISTRY = new net.minecraft.core.IdMapper<>();

    private final net.minecraft.world.level.block.state.BlockState defaultState;

    public Block() {
        this.defaultState = new net.minecraft.world.level.block.state.BlockState(this);
    }

    public net.minecraft.world.level.block.state.BlockState defaultBlockState() {
        return this.defaultState;
    }

    @Override
    public String toString() {
        return "Block@" + Integer.toHexString(System.identityHashCode(this));
    }
}
