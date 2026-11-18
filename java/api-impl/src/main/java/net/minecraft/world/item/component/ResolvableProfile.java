package net.minecraft.world.item.component;

import com.mojang.authlib.GameProfile;
import java.util.UUID;

/** Минимальный NMS {@code ResolvableProfile} для object-компонентов TAB. */
public class ResolvableProfile {
    private final UUID id;
    private final String name;
    private final GameProfile profile;

    private ResolvableProfile(UUID id, String name, GameProfile profile) {
        this.id = id;
        this.name = name;
        this.profile = profile;
    }

    public static ResolvableProfile createUnresolved(UUID id) {
        return new ResolvableProfile(id, null, null);
    }

    public static ResolvableProfile createUnresolved(String name) {
        return new ResolvableProfile(null, name, null);
    }

    public static ResolvableProfile createResolved(GameProfile profile) {
        return new ResolvableProfile(profile == null ? null : profile.getId(),
                profile == null ? null : profile.getName(), profile);
    }

    public UUID id() { return this.id; }
    public String name() { return this.name; }
    public GameProfile gameProfile() { return this.profile; }
}
