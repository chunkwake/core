package net.minecraft.network.syncher;

/** NMS {@code SyncedDataHolder} (LevelledMobs). */
public interface SyncedDataHolder {
    default SynchedEntityData getEntityData() {
        return null;
    }
}
