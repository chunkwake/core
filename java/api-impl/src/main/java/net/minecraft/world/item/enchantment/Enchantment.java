package net.minecraft.world.item.enchantment;

/** NMS-заглушка {@code Enchantment} (CommandAPI проверяет наличие). */
public class Enchantment {
    /** {@code Enchantment.getFullname(Holder, int)} (CraftEngine). */
    public static net.minecraft.network.chat.Component getFullname(
            net.minecraft.core.Holder<Enchantment> enchantment, int level) {
        return net.minecraft.network.chat.Component.empty();
    }

    public Enchantment() {
    }
}
