package net.minecraft.server.packs.repository;

/** NMS {@code PackRepository}. */
public class PackRepository {
    public java.util.List<Object> selected = new java.util.ArrayList<>();

    public PackRepository() {
    }

    public java.util.Collection<Object> getAvailablePacks() {
        return java.util.List.of();
    }

    public java.util.List<Object> getSelectedPacks() {
        return this.selected;
    }

    public java.util.Collection<String> getAvailableIds() {
        return java.util.List.of("vanilla");
    }

    public java.util.Collection<String> getSelectedIds() {
        return java.util.List.of("vanilla");
    }

    public void setSelected(java.util.Collection<String> ids) {
        this.selected = new java.util.ArrayList<>(ids);
    }

    public void reload() {
    }
}
