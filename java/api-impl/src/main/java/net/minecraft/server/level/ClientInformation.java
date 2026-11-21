package net.minecraft.server.level;

/** NMS {@code ClientInformation} (CraftEngine). */
public class ClientInformation {
    private final String language;

    public ClientInformation() {
        this("en_us");
    }

    public ClientInformation(String language) {
        this.language = language;
    }

    public String language() {
        return this.language;
    }
}
