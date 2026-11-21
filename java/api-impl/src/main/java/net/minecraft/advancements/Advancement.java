package net.minecraft.advancements;

import java.util.Map;
import java.util.Optional;

/** NMS {@code Advancement} (CraftEngine). */
public class Advancement {
    public Advancement() {
    }

    public Advancement(Optional<DisplayInfo> display, Optional<AdvancementHolder> parent,
            AdvancementRewards rewards, Map<String, Criterion> criteria,
            AdvancementRequirements requirements, boolean sendsTelemetryEvent) {
    }

    public Advancement(DisplayInfo display, AdvancementHolder parent, AdvancementRewards rewards,
            Map<String, Criterion> criteria, String[][] requirements, boolean sendsTelemetryEvent) {
    }
}
