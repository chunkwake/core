package net.minecraft.advancements;

import java.util.Map;

/** NMS {@code AdvancementProgress} (CraftEngine). */
public class AdvancementProgress {
    public AdvancementProgress() {
    }

    public void update(AdvancementRequirements requirements,
            Map<String, CriterionProgress> progress) {
    }

    public void update(Map<String, CriterionProgress> progress, String[][] requirements) {
    }

    public boolean grantProgress(String criterion) {
        return false;
    }
}
