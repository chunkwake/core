package net.minecraft.world.item;

/** Минимальный NMS {@code Item} (id предмета). */
public class Item {
    private final String id;

    public Item(String id) {
        this.id = id;
    }

    public String getId() {
        return this.id;
    }
}
