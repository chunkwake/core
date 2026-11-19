package net.minecraft.world.entity.player;

/** NMS {@code ChatVisibility} (ProtocolLib EnumWrappers). */
public enum ChatVisibility {
    FULL(0),
    SYSTEM(1),
    HIDDEN(2);

    private final int id;

    ChatVisibility(int id) {
        this.id = id;
    }

    public int getId() {
        return this.id;
    }

    public static ChatVisibility byId(int id) {
        for (ChatVisibility value : values()) {
            if (value.id == id) {
                return value;
            }
        }
        return FULL;
    }
}
