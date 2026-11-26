package dev.chunkwake.bridge;

import com.destroystokyo.paper.profile.ProfileProperty;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.net.MalformedURLException;
import java.net.URI;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.Base64;
import java.util.Locale;
import java.util.Objects;
import org.bukkit.profile.PlayerTextures;

/**
 * {@link PlayerTextures} поверх свойства {@code textures} профиля, как
 * {@code CraftPlayerTextures}. Значения хранятся на Java-стороне и лениво
 * читаются из base64-JSON свойства; любое изменение перестраивает свойство,
 * чтобы {@code PlayerProfile}, отправленный на сервер, нёс новый скин/плащ.
 *
 * <p>Подпись исходного свойства сохраняется, пока ничего не меняли. После
 * первого изменения свойство, как и у Paper, становится неподписанным, а
 * {@code timestamp} сбрасывается в 0. Неразбираемое/отсутствующее свойство не
 * бросает исключений: остаются пустые значения и {@link SkinModel#CLASSIC}.
 */
final class PlayerTexturesImpl implements PlayerTextures {
    private static final String PROPERTY = "textures";
    private static final String MINECRAFT_HOST = "textures.minecraft.net";
    private static final String MINECRAFT_PATH = "/texture/";

    private final ProfileImpl profile;
    private boolean loaded;
    private URL skin;
    private SkinModel skinModel = SkinModel.CLASSIC;
    private URL cape;
    private long timestamp;
    private boolean signed;

    PlayerTexturesImpl(ProfileImpl profile) {
        this.profile = profile;
    }

    /** Читает свойство профиля один раз (или после его замены снаружи). */
    void load() {
        if (loaded) {
            return;
        }
        loaded = true;
        if (profile == null) {
            return;
        }
        ProfileProperty property = profile.storedProperty(PROPERTY);
        if (property != null) {
            load(property);
        }
    }

    /** Сбрасывает кэш: следующее чтение снова возьмёт свойство профиля. */
    void invalidate() {
        loaded = false;
        skin = null;
        skinModel = SkinModel.CLASSIC;
        cape = null;
        timestamp = 0;
        signed = false;
    }

    private void load(ProfileProperty property) {
        skin = null;
        skinModel = SkinModel.CLASSIC;
        cape = null;
        timestamp = 0;
        signed = property.isSigned();

        JsonObject data = decode(property.getValue());
        if (data == null) {
            return;
        }
        JsonObject textures = object(data, "textures");
        if (textures != null) {
            JsonObject skinData = object(textures, "SKIN");
            if (skinData == null) {
                skinData = object(textures, "skin");
            }
            if (skinData != null) {
                skin = parseUrl(string(skinData, "url"));
                SkinModel model = loadSkinModel(skinData);
                if (model != null) {
                    skinModel = model;
                }
            }
            JsonObject capeData = object(textures, "CAPE");
            if (capeData == null) {
                capeData = object(textures, "cape");
            }
            if (capeData != null) {
                cape = parseUrl(string(capeData, "url"));
            }
        }
        timestamp = loadTimestamp(data);
    }

    @Override
    public boolean isEmpty() {
        load();
        return skin == null && cape == null;
    }

    @Override
    public void clear() {
        loaded = true;
        skin = null;
        skinModel = SkinModel.CLASSIC;
        cape = null;
        timestamp = 0;
        signed = false;
        if (profile != null) {
            profile.storeProperty(PROPERTY, null);
        }
    }

    @Override
    public URL getSkin() {
        load();
        return skin;
    }

    @Override
    public void setSkin(URL skin) {
        setSkin(skin, SkinModel.CLASSIC);
    }

    @Override
    public void setSkin(URL skin, SkinModel skinModel) {
        validateTextureUrl(skin);
        if (skinModel == null) {
            skinModel = SkinModel.CLASSIC;
        }
        load();
        if (Objects.equals(this.skin, skin) && Objects.equals(this.skinModel, skinModel)) {
            return;
        }
        this.skin = skin;
        this.skinModel = skin != null ? skinModel : SkinModel.CLASSIC;
        rebuildProperty();
    }

    @Override
    public SkinModel getSkinModel() {
        load();
        return skinModel;
    }

    @Override
    public URL getCape() {
        load();
        return cape;
    }

    @Override
    public void setCape(URL cape) {
        validateTextureUrl(cape);
        load();
        if (Objects.equals(this.cape, cape)) {
            return;
        }
        this.cape = cape;
        rebuildProperty();
    }

    @Override
    public long getTimestamp() {
        load();
        return timestamp;
    }

    @Override
    public boolean isSigned() {
        load();
        return signed;
    }

    /** Копирует значения из чужого {@link PlayerTextures}; {@code null} уже обработан в профиле. */
    void copyFrom(PlayerTextures source) {
        if (source == this) {
            return;
        }
        load();
        URL sourceSkin = source.getSkin();
        SkinModel sourceModel = source.getSkinModel();
        URL sourceCape = source.getCape();

        // Как CraftPlayerTextures.copyFrom: точное свойство (с подписью)
        // переносится только между профилями с тем же UUID и именем.
        if (source instanceof PlayerTexturesImpl other) {
            other.load();
            ProfileProperty raw = other.rawProperty();
            if (raw != null && sameIdentity(other)) {
                skin = other.skin;
                skinModel = other.skinModel;
                cape = other.cape;
                timestamp = other.timestamp;
                signed = other.signed;
                loaded = true;
                if (profile != null) {
                    profile.storeProperty(PROPERTY, raw);
                }
                return;
            }
        }

        skin = sourceSkin;
        skinModel = sourceModel != null ? sourceModel : SkinModel.CLASSIC;
        cape = sourceCape;
        loaded = true;
        rebuildProperty();
    }

    private ProfileProperty rawProperty() {
        if (profile == null) {
            return null;
        }
        ProfileProperty property = profile.storedProperty(PROPERTY);
        if (property == null) {
            return null;
        }
        // Свойство могло быть заменено снаружи после загрузки.
        load();
        return property;
    }

    private boolean sameIdentity(PlayerTexturesImpl other) {
        return profile != null && other.profile != null
                && Objects.equals(profile.getId(), other.profile.getId())
                && Objects.equals(profile.getName(), other.profile.getName());
    }

    private void rebuildProperty() {
        loaded = true;
        if (profile == null) {
            return;
        }
        if (skin == null && cape == null) {
            profile.storeProperty(PROPERTY, null);
        } else {
            JsonObject root = new JsonObject();
            JsonObject textures = new JsonObject();
            if (skin != null) {
                JsonObject value = new JsonObject();
                value.addProperty("url", skin.toExternalForm());
                if (skinModel != null && skinModel != SkinModel.CLASSIC) {
                    JsonObject metadata = new JsonObject();
                    metadata.addProperty("model", skinModel.name().toLowerCase(Locale.ROOT));
                    value.add("metadata", metadata);
                }
                textures.add("SKIN", value);
            }
            if (cape != null) {
                JsonObject value = new JsonObject();
                value.addProperty("url", cape.toExternalForm());
                textures.add("CAPE", value);
            }
            root.add("textures", textures);
            String encoded = Base64.getEncoder().encodeToString(
                    root.toString().getBytes(StandardCharsets.UTF_8));
            profile.storeProperty(PROPERTY, new ProfileProperty(PROPERTY, encoded));
        }
        timestamp = 0;
        signed = false;
    }

    private static void validateTextureUrl(URL url) {
        if (url == null) {
            return;
        }
        if (!MINECRAFT_HOST.equals(url.getHost())) {
            throw new IllegalArgumentException(
                    "Expected host '" + MINECRAFT_HOST + "' but got '" + url.getHost() + "'");
        }
        String path = url.getPath();
        if (path == null || !path.startsWith(MINECRAFT_PATH)) {
            throw new IllegalArgumentException(
                    "Expected path starting with '" + MINECRAFT_PATH + "' but got '" + path + "'");
        }
    }

    private static URL parseUrl(String value) {
        if (value == null) {
            return null;
        }
        try {
            return URI.create(value).toURL();
        } catch (IllegalArgumentException | MalformedURLException e) {
            return null;
        }
    }

    private static SkinModel loadSkinModel(JsonObject skin) {
        JsonObject metadata = object(skin, "metadata");
        if (metadata == null) {
            return null;
        }
        String model = string(metadata, "model");
        if (model == null) {
            return null;
        }
        try {
            return SkinModel.valueOf(model.toUpperCase(Locale.ROOT));
        } catch (IllegalArgumentException e) {
            return null;
        }
    }

    private static long loadTimestamp(JsonObject data) {
        JsonElement element = data.get("timestamp");
        if (element == null || !element.isJsonPrimitive()) {
            return 0;
        }
        try {
            return element.getAsJsonPrimitive().getAsLong();
        } catch (RuntimeException e) {
            return 0;
        }
    }

    private static JsonObject decode(String value) {
        if (value == null) {
            return null;
        }
        try {
            String json = new String(Base64.getDecoder().decode(value), StandardCharsets.UTF_8);
            JsonElement element = JsonParser.parseString(json);
            return element.isJsonObject() ? element.getAsJsonObject() : null;
        } catch (RuntimeException e) {
            return null;
        }
    }

    private static JsonObject object(JsonObject parent, String name) {
        JsonElement element = parent.get(name);
        return element != null && element.isJsonObject() ? element.getAsJsonObject() : null;
    }

    private static String string(JsonObject parent, String name) {
        JsonElement element = parent.get(name);
        if (element == null || !element.isJsonPrimitive()
                || !element.getAsJsonPrimitive().isString()) {
            return null;
        }
        return element.getAsString();
    }

    @Override
    public String toString() {
        return "PlayerTextures[skin=" + skin + ", skinModel=" + skinModel
                + ", cape=" + cape + ", timestamp=" + timestamp + ", signed=" + signed + "]";
    }
}
