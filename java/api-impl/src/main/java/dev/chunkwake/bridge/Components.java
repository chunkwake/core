package dev.chunkwake.bridge;

import com.destroystokyo.paper.profile.PlayerProfile;
import com.destroystokyo.paper.profile.ProfileProperty;
import io.papermc.paper.datacomponent.item.ResolvableProfile;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.UUID;
import java.util.concurrent.CompletableFuture;
import net.kyori.adventure.key.Key;
import net.kyori.adventure.text.object.PlayerHeadObjectContents;
import org.bukkit.profile.PlayerTextures;

/**
 * Строители компонентов `paper-api`, которые сервер реализует по-настоящему.
 *
 * <p>Вызывается из сгенерированного {@code ChunkwakeItemComponentTypesBridge}
 * (см. {@code java/tools/GenProviders.java}). Остальные компоненты пока
 * возвращают пустое значение, а профиль игрока и скин-патч нужны для
 * {@code Mannequin}, голов игроков и {@code ItemStack}-компонентов.
 */
public final class Components {
    private Components() {
    }

    public static ResolvableProfile.Builder resolvableProfile() {
        return new ProfileBuilder();
    }

    public static ResolvableProfile resolvableProfile(PlayerProfile profile) {
        if (profile == null) {
            return new Profile(null, null, List.of(), new SkinPatchImpl());
        }
        List<ProfileProperty> properties = new ArrayList<>();
        if (profile.getProperties() != null) {
            properties.addAll(profile.getProperties());
        }
        return new Profile(profile.getName(), profile.getId(), properties, new SkinPatchImpl());
    }

    public static ResolvableProfile.SkinPatch emptySkinPatch() {
        return new SkinPatchImpl();
    }

    public static ResolvableProfile.SkinPatchBuilder skinPatch() {
        return new SkinPatchBuilderImpl();
    }

    /** Ванильный `ResolvableProfile.Static`: имя/uuid/свойства + скин-патч. */
    static final class Profile implements ResolvableProfile {
        private final String name;
        private final UUID uuid;
        private final List<ProfileProperty> properties;
        private final ResolvableProfile.SkinPatch skinPatch;

        Profile(String name, UUID uuid, Collection<ProfileProperty> properties,
                ResolvableProfile.SkinPatch skinPatch) {
            this.name = name;
            this.uuid = uuid;
            this.properties = List.copyOf(properties);
            this.skinPatch = skinPatch;
        }

        @Override public UUID uuid() { return this.uuid; }
        @Override public String name() { return this.name; }
        @Override public Collection<ProfileProperty> properties() { return this.properties; }
        @Override public boolean dynamic() { return false; }

        @Override public CompletableFuture<PlayerProfile> resolve() {
            return CompletableFuture.completedFuture(null);
        }

        @Override public ResolvableProfile.SkinPatch skinPatch() { return this.skinPatch; }

        @Override
        public void applySkinToPlayerHeadContents(PlayerHeadObjectContents.Builder builder) {
            if (this.name != null) {
                builder.name(this.name);
            }
            if (this.uuid != null) {
                builder.id(this.uuid);
            }
            if (!this.properties.isEmpty()) {
                builder.profileProperties(this.properties.stream()
                        .map(p -> PlayerHeadObjectContents.property(p.getName(), p.getValue(),
                                p.getSignature()))
                        .toList());
            }
        }

        @Override
        public boolean equals(Object other) {
            return other instanceof Profile p
                    && java.util.Objects.equals(p.name, this.name)
                    && java.util.Objects.equals(p.uuid, this.uuid)
                    && p.properties.equals(this.properties)
                    && p.skinPatch.equals(this.skinPatch);
        }

        @Override public int hashCode() {
            return java.util.Objects.hash(this.name, this.uuid, this.properties, this.skinPatch);
        }

        @Override public String toString() {
            return "ResolvableProfile{name=" + this.name + ", uuid=" + this.uuid
                    + ", properties=" + this.properties.size() + "}";
        }
    }

    static final class ProfileBuilder implements ResolvableProfile.Builder {
        private String name;
        private UUID uuid;
        private final List<ProfileProperty> properties = new ArrayList<>();
        private ResolvableProfile.SkinPatch skinPatch;

        @Override public ResolvableProfile.Builder name(String name) {
            this.name = name;
            return this;
        }

        @Override public ResolvableProfile.Builder uuid(UUID uuid) {
            this.uuid = uuid;
            return this;
        }

        @Override public ResolvableProfile.Builder addProperty(ProfileProperty property) {
            if (property != null) {
                this.properties.add(property);
            }
            return this;
        }

        @Override public ResolvableProfile.Builder addProperties(Collection<ProfileProperty> list) {
            if (list != null) {
                this.properties.addAll(list);
            }
            return this;
        }

        @Override public ResolvableProfile.Builder skinPatch(ResolvableProfile.SkinPatch patch) {
            this.skinPatch = patch;
            return this;
        }

        @Override public ResolvableProfile.Builder skinPatch(
                java.util.function.Consumer<ResolvableProfile.SkinPatchBuilder> consumer) {
            SkinPatchBuilderImpl builder = new SkinPatchBuilderImpl();
            consumer.accept(builder);
            this.skinPatch = builder.build();
            return this;
        }

        @Override public ResolvableProfile build() {
            ResolvableProfile.SkinPatch patch = this.skinPatch == null
                    ? new SkinPatchImpl() : this.skinPatch;
            return new Profile(this.name, this.uuid, this.properties, patch);
        }
    }

    static final class SkinPatchImpl implements ResolvableProfile.SkinPatch {
        private final Key body;
        private final Key cape;
        private final Key elytra;
        private final PlayerTextures.SkinModel model;

        SkinPatchImpl() {
            this(null, null, null, null);
        }

        SkinPatchImpl(Key body, Key cape, Key elytra, PlayerTextures.SkinModel model) {
            this.body = body;
            this.cape = cape;
            this.elytra = elytra;
            this.model = model;
        }

        @Override public Key body() { return this.body; }
        @Override public Key cape() { return this.cape; }
        @Override public Key elytra() { return this.elytra; }
        @Override public PlayerTextures.SkinModel model() { return this.model; }

        @Override public boolean isEmpty() {
            return this.body == null && this.cape == null && this.elytra == null && this.model == null;
        }

        @Override
        public boolean equals(Object other) {
            return other instanceof SkinPatchImpl p
                    && java.util.Objects.equals(p.body, this.body)
                    && java.util.Objects.equals(p.cape, this.cape)
                    && java.util.Objects.equals(p.elytra, this.elytra)
                    && p.model == this.model;
        }

        @Override public int hashCode() {
            return java.util.Objects.hash(this.body, this.cape, this.elytra, this.model);
        }

        @Override public String toString() {
            return "SkinPatch{body=" + this.body + ", cape=" + this.cape + ", elytra=" + this.elytra
                    + ", model=" + this.model + "}";
        }
    }

    static final class SkinPatchBuilderImpl implements ResolvableProfile.SkinPatchBuilder {
        private Key body;
        private Key cape;
        private Key elytra;
        private PlayerTextures.SkinModel model;

        @Override public ResolvableProfile.SkinPatchBuilder body(Key body) {
            this.body = body;
            return this;
        }

        @Override public ResolvableProfile.SkinPatchBuilder cape(Key cape) {
            this.cape = cape;
            return this;
        }

        @Override public ResolvableProfile.SkinPatchBuilder elytra(Key elytra) {
            this.elytra = elytra;
            return this;
        }

        @Override public ResolvableProfile.SkinPatchBuilder model(PlayerTextures.SkinModel model) {
            this.model = model;
            return this;
        }

        @Override public ResolvableProfile.SkinPatch build() {
            return new SkinPatchImpl(this.body, this.cape, this.elytra, this.model);
        }
    }
}
