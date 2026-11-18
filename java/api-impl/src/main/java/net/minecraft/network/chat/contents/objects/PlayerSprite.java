package net.minecraft.network.chat.contents.objects;

import net.minecraft.world.item.component.ResolvableProfile;

/** NMS object-компонент головы игрока. */
public final class PlayerSprite implements ObjectInfo {
    private final ResolvableProfile profile;
    private final boolean showHat;

    public PlayerSprite(ResolvableProfile profile, boolean showHat) {
        this.profile = profile;
        this.showHat = showHat;
    }

    public ResolvableProfile profile() { return this.profile; }
    public boolean showHat() { return this.showHat; }
}
