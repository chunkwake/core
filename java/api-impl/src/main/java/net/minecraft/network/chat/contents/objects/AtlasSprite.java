package net.minecraft.network.chat.contents.objects;

import net.minecraft.resources.Identifier;

/** NMS-спрайт атласа для object-компонента. */
public final class AtlasSprite implements ObjectInfo {
    private final Identifier atlas;
    private final Identifier sprite;

    public AtlasSprite(Identifier atlas, Identifier sprite) {
        this.atlas = atlas;
        this.sprite = sprite;
    }

    public Identifier atlas() { return this.atlas; }
    public Identifier sprite() { return this.sprite; }
}
