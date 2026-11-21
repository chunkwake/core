package net.minecraft.world.entity.player;

import net.minecraft.world.entity.LivingEntity;

/** NMS {@code Player} (InvSee++, Axiom). */
public class Player extends LivingEntity {
    private String name;

    public Player() {
    }

    public Player(String name) {
        this.name = name;
    }

    /** Поле, которое ищет CraftEngine (PlayerProxy#getContainerMenu). */
    public net.minecraft.world.inventory.AbstractContainerMenu containerMenu;

    /** Поле, которое ищет CraftEngine (PlayerProxy#getInventoryMenu). */
    public final net.minecraft.world.inventory.InventoryMenu inventoryMenu =
            new net.minecraft.world.inventory.InventoryMenu();

    /** Поле, которое ищет CraftEngine (PlayerProxy#setInventory). */
    private Object inventory;

    /** Игрок с заданным UUID (CraftPlayer.getHandle() должен совпадать). */
    protected Player(java.util.UUID uuid, String name) {
        super(uuid);
        this.name = name;
    }

    public String getName() {
        return this.name;
    }

    public net.minecraft.world.entity.EntityType<?> getType() {
        return net.minecraft.world.entity.EntityTypes.PLAYER;
    }
}
