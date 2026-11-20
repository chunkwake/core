package net.minecraft.world.entity.animal.axolotl;

/** NMS {@code Axolotl}: FancyNpcs использует вложенный Variant. */
public class Axolotl {
    public enum Variant {
        LUCY(0, "lucy", true),
        WILD(1, "wild", true),
        GOLD(2, "gold", false),
        CYAN(3, "cyan", false),
        BLUE(4, "blue", false),
        DEFAULT(4, "blue", false);

        private final int id;
        private final String name;
        private final boolean common;

        Variant(int id, String name, boolean common) {
            this.id = id;
            this.name = name;
            this.common = common;
        }

        public int getId() {
            return this.id;
        }

        public String getName() {
            return this.name;
        }

        public String getSerializedName() {
            return this.name;
        }

        public boolean isCommon() {
            return this.common;
        }

        public static Variant byId(int id) {
            for (Variant variant : values()) {
                if (variant.id == id) {
                    return variant;
                }
            }
            return LUCY;
        }
    }
}
