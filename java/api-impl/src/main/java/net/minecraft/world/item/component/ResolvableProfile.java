package net.minecraft.world.item.component;

/**
 * NMS {@code ResolvableProfile} (26.x): профиль головы.
 *
 * <p>EconomyShopGUI ищет в этом классе поле типа
 * {@code com.mojang.authlib.GameProfile}, чтобы читать/писать владельца
 * черепа.
 */
public class ResolvableProfile {
    public com.mojang.authlib.GameProfile profile;

    public ResolvableProfile() {
        this.profile = new com.mojang.authlib.GameProfile(
                java.util.UUID.randomUUID(), "Chunkwake");
    }

    public ResolvableProfile(com.mojang.authlib.GameProfile profile) {
        this.profile = profile;
    }

    public com.mojang.authlib.GameProfile profile() {
        return this.profile;
    }
}
