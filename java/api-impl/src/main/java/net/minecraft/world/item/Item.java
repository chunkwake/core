package net.minecraft.world.item;

/** Минимальный NMS {@code Item} (id предмета). */
public class Item {
    /** {@code Item.use(Level, Player, InteractionHand)} (CraftEngine). */
    public net.minecraft.world.InteractionResult use(net.minecraft.world.level.Level level,
            net.minecraft.world.entity.player.Player player,
            net.minecraft.world.InteractionHand hand) {
        return net.minecraft.world.InteractionResult.PASS;
    }

    private final String id;

    public Item(String id) {
        this.id = id;
    }

    public String getId() {
        return this.id;
    }
}
