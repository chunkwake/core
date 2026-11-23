package net.minecraft;

/** NMS {@code WorldVersion}. */
public interface WorldVersion {
    String name();

    net.minecraft.world.level.storage.DataVersion dataVersion();

    int protocolVersion();

    /** Paper 26.3: resource/data pack format for the current version. */
    default net.minecraft.server.packs.metadata.pack.PackFormat packVersion(
            net.minecraft.server.packs.PackType type) {
        return type == net.minecraft.server.packs.PackType.SERVER_DATA
                ? net.minecraft.server.packs.metadata.pack.PackFormat.of(121, 0)
                : net.minecraft.server.packs.metadata.pack.PackFormat.of(97, 1);
    }

    default String getName() {
        return this.name();
    }

    default int getDataVersion() {
        return this.dataVersion().version();
    }

    default int getProtocolVersion() {
        return this.protocolVersion();
    }

    /** Текущая версия мира (26.3). */
    final class Current implements WorldVersion {
        public static final Current INSTANCE = new Current();

        private Current() {
        }

        @Override
        public String name() {
            return SharedConstants.VERSION;
        }

        @Override
        public net.minecraft.world.level.storage.DataVersion dataVersion() {
            return new net.minecraft.world.level.storage.DataVersion(5023);
        }

        @Override
        public int protocolVersion() {
            return SharedConstants.getProtocolVersion();
        }
    }
}
