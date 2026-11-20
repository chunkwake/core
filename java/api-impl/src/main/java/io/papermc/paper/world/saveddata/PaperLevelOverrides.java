package io.papermc.paper.world.saveddata;

import net.minecraft.world.level.storage.LevelData;

/**
 * Paper-совместимость: в Paper поле {@code ServerLevel.serverLevelData}
 * имеет этот тип. squaremap ({@code PaperMapWorld.getSpawnPos}) читает
 * поле рефлексией/напрямую и берёт {@code getRespawnData().pos()}.
 */
public class PaperLevelOverrides implements LevelData {
    private final RespawnData respawnData;

    public PaperLevelOverrides() {
        this(RespawnData.DEFAULT);
    }

    public PaperLevelOverrides(RespawnData respawnData) {
        this.respawnData = respawnData == null ? RespawnData.DEFAULT : respawnData;
    }

    @Override
    public RespawnData getRespawnData() {
        return this.respawnData;
    }
}
