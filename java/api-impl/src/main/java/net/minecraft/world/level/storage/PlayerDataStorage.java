package net.minecraft.world.level.storage;

import java.io.File;

/** Минимальный NMS {@code PlayerDataStorage} (InvSee++: offline-данные). */
public class PlayerDataStorage {
    private File playerDir = new File(".");

    public File getPlayerDir() {
        return this.playerDir;
    }

    public File getPlayerDataFolder() {
        return this.playerDir;
    }

    public void setPlayerDir(File dir) {
        this.playerDir = dir;
    }
}
