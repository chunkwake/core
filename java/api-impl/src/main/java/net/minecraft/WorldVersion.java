package net.minecraft;

/** NMS {@code WorldVersion}. */
public interface WorldVersion {
    String name();

    int dataVersion();

    int protocolVersion();

    default String getName() {
        return this.name();
    }

    default int getDataVersion() {
        return this.dataVersion();
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
        public int dataVersion() {
            return 4556;
        }

        @Override
        public int protocolVersion() {
            return SharedConstants.getProtocolVersion();
        }
    }
}
