package net.minecraft.world.level;

/** NMS {@code GameType}. */
public enum GameType {
    SURVIVAL(0, "survival"),
    CREATIVE(1, "creative"),
    ADVENTURE(2, "adventure"),
    SPECTATOR(3, "spectator");

    private final int id;
    private final String name;

    GameType(int id, String name) {
        this.id = id;
        this.name = name;
    }

    public int getId() {
        return this.id;
    }

    public String getName() {
        return this.name;
    }

    public boolean isSurvival() {
        return this == SURVIVAL;
    }

    public boolean isCreative() {
        return this == CREATIVE;
    }

    public boolean isBlockPlacingRestricted() {
        return this == ADVENTURE || this == SPECTATOR;
    }

    public static GameType byId(int id) {
        return switch (id) {
            case 1 -> CREATIVE;
            case 2 -> ADVENTURE;
            case 3 -> SPECTATOR;
            default -> SURVIVAL;
        };
    }
}
