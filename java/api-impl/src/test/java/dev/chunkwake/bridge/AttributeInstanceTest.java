package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.function.Function;
import org.bukkit.NamespacedKey;
import org.bukkit.attribute.Attribute;
import org.bukkit.attribute.AttributeInstance;
import org.bukkit.attribute.AttributeModifier;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.Test;

/** {@link AttributeInstanceImpl} против поддельного Rust-сервера с той же логикой запроса {@code attr}. */
class AttributeInstanceTest {
    private static final UUID ID = new UUID(1, 2);
    private final Function<String, String> saved = Registries.source;
    private final List<String> sent = new ArrayList<>();

    @AfterEach
    void restore() {
        Registries.source = saved;
    }

    /** Один атрибут {@code minecraft:block_break_speed} с базой 1 у сущности {@link #ID}. */
    private AttributeInstance instance() {
        Attribute attribute = Attribute.BLOCK_BREAK_SPEED; // реестр — до подмены источника
        double[] base = {1.0};
        Map<String, String> mods = new LinkedHashMap<>();
        Registries.source = line -> {
            sent.add(line);
            String[] f = line.split(" ");
            if (!f[0].equals("attr") || !f[1].equals(ID.toString()) || !f[2].equals("minecraft:block_break_speed")) {
                return "";
            }
            return switch (f[3]) {
                case "get" -> {
                    double added = base[0], mulBase = 0, total = 1;
                    StringBuilder tail = new StringBuilder();
                    for (var e : mods.entrySet()) {
                        String[] m = e.getValue().split(" ");
                        double v = Double.parseDouble(m[0]);
                        switch (m[1]) {
                            case "0" -> added += v;
                            case "1" -> mulBase += v;
                            default -> total *= 1 + v;
                        }
                        tail.append("\n").append(e.getKey()).append(" ").append(e.getValue());
                    }
                    yield base[0] + " " + added * (1 + mulBase) * total + " 1.0" + tail;
                }
                case "base" -> {
                    base[0] = Double.parseDouble(f[4]);
                    yield "1";
                }
                case "add" -> mods.putIfAbsent(f[4], f[5] + " " + f[6]) == null ? "1" : "";
                case "remove" -> mods.remove(f[4]) != null ? "1" : "";
                default -> "";
            };
        };
        return AttributeInstanceImpl.of(() -> ID, attribute);
    }

    @Test
    void modifiersRoundTripThroughServer() {
        RegistriesTest.vanillaSourceIfPresent();
        AttributeInstance a = instance();
        assertEquals(1.0, a.getValue());
        NamespacedKey key = new NamespacedKey("veinminer", "slow");
        a.addModifier(new AttributeModifier(key, -0.5, AttributeModifier.Operation.ADD_SCALAR));
        assertEquals(0.5, a.getValue(), 1e-9);
        assertEquals("attr " + ID + " minecraft:block_break_speed add veinminer:slow -0.5 1", sent.get(sent.size() - 2));
        AttributeModifier back = a.getModifier(key);
        assertEquals(AttributeModifier.Operation.ADD_SCALAR, back.getOperation());
        assertEquals(-0.5, back.getAmount());
        assertThrows(IllegalArgumentException.class, () -> a.addModifier(back));
        a.removeModifier(key);
        assertEquals(List.of(), List.copyOf(a.getModifiers()));
        a.setBaseValue(2.0);
        assertEquals(2.0, a.getBaseValue());
        assertEquals(1.0, a.getDefaultValue());
    }

    @Test
    void missingAttributeIsNull() {
        RegistriesTest.vanillaSourceIfPresent();
        instance();
        assertNull(AttributeInstanceImpl.of(() -> ID, Attribute.MAX_HEALTH));
    }
}
