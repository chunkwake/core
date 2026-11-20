package net.minecraft;

/** NMS {@code WorldVersion}. */
public interface WorldVersion {
    String name();

    net.minecraft.world.level.storage.DataVersion dataVersion();

    int protocolVersion();

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
