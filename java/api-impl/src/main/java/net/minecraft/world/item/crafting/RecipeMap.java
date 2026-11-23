package net.minecraft.world.item.crafting;

import com.google.common.collect.Multimap;
import java.util.LinkedHashMap;
import java.util.Map;

/** NMS {@code RecipeMap}: стаб для CraftEngine RecipeMapProxy. */
public class RecipeMap {
    private final Multimap<Object, Object> byType;
    private final Map<Object, Object> byKey;
    private final Map<Object, Object> byName;

    public RecipeMap(Multimap<?, ?> byType, Map<?, ?> byName) {
        this.byType = (Multimap<Object, Object>) byType;
        this.byName = (Map<Object, Object>) byName;
        this.byKey = new LinkedHashMap<>();
    }

    public RecipeMap(Multimap<?, ?> byType, LinkedHashMap<?, ?> byName) {
        this(byType, (Map<?, ?>) byName);
    }
}
