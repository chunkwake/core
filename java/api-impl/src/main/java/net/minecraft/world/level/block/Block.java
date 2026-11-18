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

    public static int getId(net.minecraft.world.level.block.state.BlockState state) {
        return BLOCK_STATE_REGISTRY.getId(state);
    }

    public static net.minecraft.world.level.block.state.BlockState stateById(int id) {
        return BLOCK_STATE_REGISTRY.byId(id);
    }

    public net.minecraft.world.level.block.state.StateDefinition<Block,
            net.minecraft.world.level.block.state.BlockState> getStateDefinition() {
        return new net.minecraft.world.level.block.state.StateDefinition<>(this, java.util.List.of());
    }

    public String getDescriptionId() {
        return "block.minecraft.air";
    }

    public String getName() {
        return "Air";
    }

    @Override
    public String toString() {
        return "Block@" + Integer.toHexString(System.identityHashCode(this));
    }
}
