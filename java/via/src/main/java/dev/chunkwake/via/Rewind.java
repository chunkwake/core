package dev.chunkwake.via;

import com.viaversion.viarewind.api.ViaRewindPlatform;
import java.io.File;
import java.util.logging.Logger;

/** ViaRewind: клиенты 1.7–1.8 (поверх ViaBackwards). */
final class Rewind implements ViaRewindPlatform {
    private final File dataFolder;
    private final Logger logger = Logger.getLogger("ViaRewind");

    Rewind(File dataFolder) {
        this.dataFolder = dataFolder;
    }

    @Override
    public Logger getLogger() {
        return logger;
    }

    @Override
    public File getDataFolder() {
        return dataFolder;
    }
}
