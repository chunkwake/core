package net.minecraft.world.level.block;

import java.util.List;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.StateDefinition;

/**
 * NMS {@code Block}. Реестр состояний наполняется из настоящего
 * {@code reports/blocks.json} (id состояний = сетевые id), поэтому
 * плагины (Orebfuscator, Axiom) видят ванильные блоки и их состояния.
 */
public class Block {
    public static final net.minecraft.core.IdMapper<BlockState> BLOCK_STATE_REGISTRY =
            new net.minecraft.core.IdMapper<>();

    private final String name;
    private BlockState defaultState;
    private StateDefinition<Block, BlockState> definition;

    public Block() {
        this("minecraft:air");
    }

    public Block(String name) {
        this.name = name == null ? "minecraft:air" : name;
        this.defaultState = new BlockState(this);
    }

    /** Заполняет состояния блока (вызывается бутстрапом). */
    public void defineStates(List<BlockState> states, BlockState defaultState) {
        this.definition = new StateDefinition<>(this, List.of(), states);
        if (defaultState != null) {
            this.defaultState = defaultState;
        }
    }

    public BlockState defaultBlockState() {
        return this.defaultState;
    }

    public String name() {
        return this.name;
    }

    public static int getId(BlockState state) {
        return BLOCK_STATE_REGISTRY.getId(state);
    }

    public static BlockState stateById(int id) {
        return BLOCK_STATE_REGISTRY.byId(id);
    }

    public StateDefinition<Block, BlockState> getStateDefinition() {
        return this.definition != null ? this.definition
                : new StateDefinition<>(this, List.of());
    }

    public String getDescriptionId() {
        return "block." + this.name.replace(':', '.');
    }

    public String getName() {
        String path = this.name.contains(":")
                ? this.name.substring(this.name.indexOf(':') + 1) : this.name;
        return path;
    }

    @Override
    public String toString() {
        return this.name;
    }
}
