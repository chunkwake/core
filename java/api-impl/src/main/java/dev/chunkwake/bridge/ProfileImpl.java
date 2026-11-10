package dev.chunkwake.bridge;

import com.destroystokyo.paper.profile.PlayerProfile;
import com.destroystokyo.paper.profile.ProfileProperty;
import java.util.Collection;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.CompletableFuture;
import org.bukkit.profile.PlayerTextures;

/**
 * {@link PlayerProfile} как {@code CraftPlayerProfile}: имя, UUID и свойства
 * ({@code textures} от session server). Запросов к Mojang нет: профиль
 * «полон», когда есть имя и UUID, — как {@code isComplete} у Paper.
 */
final class ProfileImpl implements PlayerProfile {
    private String name;
    private UUID id;
    /** По имени свойства, как {@code PropertyMap} профиля: одно на имя. */
    private final Map<String, ProfileProperty> properties = new LinkedHashMap<>();

    ProfileImpl(UUID id, String name) {
        this.id = id;
        this.name = name;
    }

    @Override public String getName() { return name; }

    @Override
    public String setName(String name) {
        String old = this.name;
        this.name = name;
        return old;
    }

    @Override public UUID getId() { return id; }
    @Override public UUID getUniqueId() { return id; }

    @Override
    public UUID setId(UUID id) {
        UUID old = this.id;
        this.id = id;
        return old;
    }

    @Override
    public PlayerTextures getTextures() {
        return Stub.of(PlayerTextures.class, new Object());
    }

    @Override public void setTextures(PlayerTextures textures) {}

    @Override public Set<ProfileProperty> getProperties() { return new LinkedHashSet<>(properties.values()); }
    @Override public boolean hasProperty(String property) { return properties.containsKey(property); }

    @Override
    public void setProperty(ProfileProperty property) {
        properties.put(property.getName(), property);
    }

    @Override
    public void setProperties(Collection<ProfileProperty> properties) {
        properties.forEach(this::setProperty);
    }

    @Override public boolean removeProperty(String property) { return properties.remove(property) != null; }
    @Override public void clearProperties() { properties.clear(); }

    @Override
    public boolean isComplete() {
        return id != null && name != null && !name.isBlank();
    }

    @Override public boolean completeFromCache() { return isComplete(); }
    @Override public boolean completeFromCache(boolean onlineMode) { return isComplete(); }
    @Override public boolean completeFromCache(boolean lookupUUID, boolean onlineMode) { return isComplete(); }

    @Override
    public boolean complete(boolean textures) {
        return isComplete() && (!textures || hasProperty("textures"));
    }

    @Override
    public boolean complete(boolean textures, boolean onlineMode) {
        return complete(textures);
    }

    @Override
    public CompletableFuture<PlayerProfile> update() {
        return CompletableFuture.completedFuture(clone());
    }

    @Override
    public ProfileImpl clone() {
        ProfileImpl copy = new ProfileImpl(id, name);
        copy.properties.putAll(properties);
        return copy;
    }

    @Override
    public Map<String, Object> serialize() {
        Map<String, Object> out = new LinkedHashMap<>();
        if (id != null) {
            out.put("uniqueId", id.toString());
        }
        if (name != null) {
            out.put("name", name);
        }
        return out;
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof ProfileImpl p && Objects.equals(id, p.id) && Objects.equals(name, p.name)
                && properties.equals(p.properties);
    }

    @Override
    public int hashCode() {
        return Objects.hash(id, name, properties);
    }

    @Override
    public String toString() {
        return "CraftPlayerProfile [uuid=" + id + ", name=" + name + ", properties=" + properties.values() + "]";
    }
}
