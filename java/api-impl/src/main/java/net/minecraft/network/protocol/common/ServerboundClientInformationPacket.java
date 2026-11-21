package net.minecraft.network.protocol.common;

import net.minecraft.network.protocol.Packet;
import net.minecraft.server.level.ClientInformation;

/** NMS {@code ServerboundClientInformationPacket} (CraftEngine). */
public class ServerboundClientInformationPacket implements Packet {
    /** Поля, которые ищет CraftEngine (getLanguage/getInformation). */
    private String language = "en_us";
    private ClientInformation information = new ClientInformation();

    public ServerboundClientInformationPacket() {
    }

    public String getLanguage() {
        return this.information == null ? "en_us" : this.information.language();
    }

    public ClientInformation getInformation() {
        return this.information;
    }
}
