package net.minecraft.world.item;

/** NMS {@code Items}: ванильные предметы, используемые CraftEngine. */
public final class Items {
    public static final Item AIR = new Item("minecraft:air");
    public static final Item WATER_BUCKET = new Item("minecraft:water_bucket");
    public static final Item BARRIER = new Item("minecraft:barrier");
    public static final Item DEBUG_STICK = new Item("minecraft:debug_stick");
    public static final Item WRITABLE_BOOK = new Item("minecraft:writable_book");
    public static final Item FIRE_CHARGE = new Item("minecraft:fire_charge");
    public static final Item WIND_CHARGE = new Item("minecraft:wind_charge");

    private Items() {
    }

    public static Item getWindCharge() {
        return WIND_CHARGE;
    }
}
