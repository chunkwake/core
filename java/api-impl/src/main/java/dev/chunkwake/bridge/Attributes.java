package dev.chunkwake.bridge;

import java.util.LinkedHashMap;
import java.util.Locale;
import java.util.Map;
import org.bukkit.NamespacedKey;
import org.bukkit.attribute.Attribute;

/**
 * Атрибуты 26.3 + legacy-алиасы.
 *
 * <p>Paper {@code Attribute} — интерфейс; его статические поля и
 * {@code Attribute.valueOf(...)} идут через {@code Unsafe.get(...)}. Раньше
 * там возвращался прокси, и EliteMobs падал на
 * {@code ClassCastException: $Proxy cannot be cast to Attribute}.
 */
final class Attributes {
    private static final Map<NamespacedKey, AttributeImpl> BY_KEY = new LinkedHashMap<>();

    private Attributes() {
    }

    private static void add(String modern, String legacy, double def) {
        AttributeImpl attr = new AttributeImpl(
                NamespacedKey.minecraft(modern), BY_KEY.size(), def);
        BY_KEY.put(attr.getKey(), attr);
        if (legacy != null) {
            BY_KEY.put(NamespacedKey.minecraft(legacy), attr);
        }
    }

    static {
        add("max_health", "generic_max_health", 20.0);
        add("follow_range", "generic_follow_range", 32.0);
        add("knockback_resistance", "generic_knockback_resistance", 0.0);
        add("movement_speed", "generic_movement_speed", 0.7);
        add("flying_speed", "generic_flying_speed", 0.4);
        add("attack_damage", "generic_attack_damage", 2.0);
        add("attack_knockback", "generic_attack_knockback", 0.0);
        add("attack_speed", "generic_attack_speed", 4.0);
        add("armor", "generic_armor", 0.0);
        add("armor_toughness", "generic_armor_toughness", 0.0);
        add("fall_damage_multiplier", "generic_fall_damage_multiplier", 1.0);
        add("luck", "generic_luck", 0.0);
        add("max_absorption", "generic_max_absorption", 0.0);
        add("safe_fall_distance", "generic_safe_fall_distance", 3.0);
        add("scale", "generic_scale", 1.0);
        add("step_height", "generic_step_height", 0.6);
        add("gravity", "generic_gravity", 0.08);
        add("jump_strength", "generic_jump_strength", 0.42);
        add("burning_time", "generic_burning_time", 1.0);
        add("camera_distance", "generic_camera_distance", 4.0);
        add("explosion_knockback_resistance", "generic_explosion_knockback_resistance", 0.0);
        add("movement_efficiency", "generic_movement_efficiency", 0.0);
        add("oxygen_bonus", "generic_oxygen_bonus", 0.0);
        add("water_movement_efficiency", "generic_water_movement_efficiency", 0.0);
        add("block_break_speed", "generic_block_break_speed", 1.0);
        add("mining_efficiency", "generic_mining_efficiency", 0.0);
        add("sneaking_speed", "generic_sneaking_speed", 0.3);
        add("submerged_mining_speed", "generic_submerged_mining_speed", 0.2);
        add("sweeping_damage_ratio", "generic_sweeping_damage_ratio", 0.0);
        add("waypoint_transmit_range", "generic_waypoint_transmit_range", 60_000_000.0);
        add("block_interaction_range", "generic_block_interaction_range", 4.5);
        add("entity_interaction_range", "generic_entity_interaction_range", 3.0);
    }

    static Attribute byKey(NamespacedKey key) {
        if (key == null) {
            return null;
        }
        Attribute attr = BY_KEY.get(key);
        if (attr == null && key.getNamespace().equals("minecraft")) {
            attr = BY_KEY.get(NamespacedKey.minecraft(key.getKey().toLowerCase(Locale.ROOT)));
        }
        return attr;
    }
}
