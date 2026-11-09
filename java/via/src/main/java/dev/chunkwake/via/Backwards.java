package dev.chunkwake.via;

import com.viaversion.viabackwards.api.ViaBackwardsPlatform;
import java.io.File;
import java.util.logging.Logger;

/** ViaBackwards: клиенты старее сервера. */
final class Backwards implements ViaBackwardsPlatform {
    private final File dataFolder;
    private final Logger logger = Logger.getLogger("ViaBackwards");

    Backwards(File dataFolder) {
        this.dataFolder = dataFolder;
    }

    @Override
    public Logger getLogger() {
        return logger;
    }

    @Override
    public void disable() {}

    @Override
    public File getDataFolder() {
        return dataFolder;
    }
}
