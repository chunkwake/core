package net.minecraft.world.entity.monster.illager;

import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.monster.Monster;

/** NMS {@code SpellcasterIllager}. */
public abstract class SpellcasterIllager extends Monster {
    public enum IllagerSpell {
        NONE,
        SUMMON_VEX,
        FANGS,
        WOLOLO,
        DISAPPEAR
    }

    public SpellcasterIllager(EntityType<?> type, net.minecraft.world.level.Level level) {
        super(type, level);
    }

    public SpellcasterIllager() {
        super();
    }
}
