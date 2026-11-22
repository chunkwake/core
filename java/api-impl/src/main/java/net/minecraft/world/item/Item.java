package net.minecraft.world.item;

/** Минимальный NMS {@code Item} (id предмета). */
public class Item implements ProjectileItem, net.minecraft.world.level.ItemLike {
    /** {@code Item.canBeDepleted()} (CraftEngine). */
    public boolean canBeDepleted() {
        return true;
    }

    /** Поле, которое ищет CraftEngine (ItemProxy#getBuiltInRegistryHolder). */
    public final net.minecraft.core.Holder.Reference<Item> builtInRegistryHolder =
            new net.minecraft.core.Holder.Reference<>(this);

    /** {@code Item.getDescriptionId()} (CraftEngine). */
    public String getDescriptionId() {
        return "item.minecraft.air";
    }

    /** Поле, которое ищет CraftEngine (ItemProxy#setComponents). */
    private net.minecraft.core.component.DataComponentMap components;

    /** {@code Item.components()} (CraftEngine). */
    public net.minecraft.core.component.DataComponentMap components() {
        if (this.components == null) {
            this.components = new net.minecraft.core.component.PatchedDataComponentMap(null);
        }
        return this.components;
    }

    /** {@code Item.use(Level, Player, InteractionHand)} (CraftEngine). */
    public net.minecraft.world.InteractionResult use(net.minecraft.world.level.Level level,
            net.minecraft.world.entity.player.Player player,
            net.minecraft.world.InteractionHand hand) {
        return net.minecraft.world.InteractionResult.PASS;
    }

    private final String id;

    @Override
    public Item asItem() {
        return this;
    }

    public Item(String id) {
        this.id = id;
    }

    public String getId() {
        return this.id;
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof Item item && java.util.Objects.equals(this.getId(), item.getId());
    }

    @Override
    public int hashCode() {
        return java.util.Objects.hashCode(this.getId());
    }
}
