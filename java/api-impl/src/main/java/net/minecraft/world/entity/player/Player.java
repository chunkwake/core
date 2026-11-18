package net.minecraft.world.entity.player;

/** Минимальный NMS {@code Player} (InvSee++ проверяет иерархию). */
public class Player {
    private String name;

    public Player() {
    }

    public Player(String name) {
        this.name = name;
    }

    public String getName() {
        return this.name;
    }
}
