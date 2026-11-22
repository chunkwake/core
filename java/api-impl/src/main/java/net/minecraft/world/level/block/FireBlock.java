package net.minecraft.world.level.block;

/** NMS {@code FireBlock}: горючесть блоков. */
public class FireBlock extends BaseFireBlock {
    public final it.unimi.dsi.fastutil.objects.Object2IntMap<Block> igniteOdds =
            new it.unimi.dsi.fastutil.objects.Object2IntOpenHashMap<>();
    public final it.unimi.dsi.fastutil.objects.Object2IntMap<Block> burnOdds =
            new it.unimi.dsi.fastutil.objects.Object2IntOpenHashMap<>();

    public FireBlock() {
    }

    public FireBlock(net.minecraft.world.level.block.state.BlockBehaviour.Properties properties) {
        super(properties);
    }

    public void setFlammable(Block block, int igniteOdds, int burnOdds) {
        this.igniteOdds.put(block, igniteOdds);
        this.burnOdds.put(block, burnOdds);
    }
}
