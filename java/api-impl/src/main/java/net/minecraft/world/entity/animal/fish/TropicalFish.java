package net.minecraft.world.entity.animal.fish;

/** NMS {@code TropicalFish}: FancyNpcs использует Pattern/Base. */
public class TropicalFish {
    public enum Pattern {
        KOB,
        SUNSTREAK,
        SNOOPER,
        DASHER,
        BRINELY,
        SPOTTY,
        FLOPPER,
        STRIPEY,
        GLITTER,
        BLOCKFISH,
        BETTY,
        CLAYFISH;

        public static Pattern byId(int id) {
            Pattern[] all = values();
            return id >= 0 && id < all.length ? all[id] : KOB;
        }
    }

    public enum Base {
        SMALL,
        LARGE;

        public static Base byId(int id) {
            Base[] all = values();
            return id >= 0 && id < all.length ? all[id] : SMALL;
        }
    }
}
