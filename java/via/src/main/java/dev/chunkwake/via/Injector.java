package dev.chunkwake.via;

import com.viaversion.viaversion.api.platform.ViaInjector;
import com.viaversion.viaversion.api.protocol.version.ProtocolVersion;
import com.viaversion.viaversion.libs.gson.JsonObject;
import com.viaversion.viaversion.platform.ViaCodecHandler;

/**
 * Внедряться некуда: каналы соединений создаёт {@link ChunkwakeVia}.
 * Кодер и декодер Via в них — один {@link ViaCodecHandler}; по этим именам
 * Via пишет пакеты в обход перевода ({@code sendRawPacket}).
 */
final class Injector implements ViaInjector {
    /** Протокол Chunkwake (26.3). */
    static final int SERVER_PROTOCOL = 777;

    @Override
    public void inject() {}

    @Override
    public void uninject() {}

    @Override
    public ProtocolVersion getServerProtocolVersion() {
        return ProtocolVersion.getProtocol(SERVER_PROTOCOL);
    }

    @Override
    public String getEncoderName() {
        return ViaCodecHandler.NAME;
    }

    @Override
    public String getDecoderName() {
        return ViaCodecHandler.NAME;
    }

    @Override
    public JsonObject getDump() {
        JsonObject dump = new JsonObject();
        dump.addProperty("injector", "chunkwake");
        return dump;
    }
}
