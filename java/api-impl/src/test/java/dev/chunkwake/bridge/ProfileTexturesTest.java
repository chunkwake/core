package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.destroystokyo.paper.profile.ProfileProperty;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.net.URI;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.Base64;
import java.util.UUID;
import org.bukkit.profile.PlayerTextures;
import org.bukkit.profile.PlayerTextures.SkinModel;
import org.junit.jupiter.api.Test;

/** {@link PlayerTexturesImpl} через {@link ProfileImpl}, без Bridge и нативного сервера. */
class ProfileTexturesTest {
    private static final UUID ID = new UUID(1, 2);

    private static URL texture(String name) throws Exception {
        return URI.create("http://textures.minecraft.net/texture/" + name).toURL();
    }

    private static ProfileProperty texturesProperty(ProfileImpl profile) {
        return profile.getProperties().stream()
                .filter(p -> p.getName().equals("textures"))
                .findFirst()
                .orElse(null);
    }

    private static JsonObject decode(ProfileProperty property) {
        String json = new String(Base64.getDecoder().decode(property.getValue()),
                StandardCharsets.UTF_8);
        return JsonParser.parseString(json).getAsJsonObject();
    }

    private static String encode(String json) {
        return Base64.getEncoder().encodeToString(json.getBytes(StandardCharsets.UTF_8));
    }

    @Test
    void defaultsAreEmptyClassicAndStable() {
        ProfileImpl profile = new ProfileImpl(ID, "Steve");
        PlayerTextures textures = profile.getTextures();
        assertNotNull(textures);
        assertSame(textures, profile.getTextures());
        assertTrue(textures.isEmpty());
        assertNull(textures.getSkin());
        assertNull(textures.getCape());
        assertEquals(SkinModel.CLASSIC, textures.getSkinModel());
        assertEquals(0L, textures.getTimestamp());
        assertFalse(textures.isSigned());
        assertFalse(profile.hasProperty("textures"));
    }

    @Test
    void setSkinAndCapeRebuildTexturesProperty() throws Exception {
        ProfileImpl profile = new ProfileImpl(ID, "Steve");
        PlayerTextures textures = profile.getTextures();
        URL skin = texture("skin");
        URL cape = texture("cape");

        textures.setSkin(skin, SkinModel.SLIM);
        assertEquals(skin, textures.getSkin());
        assertEquals(SkinModel.SLIM, textures.getSkinModel());
        assertFalse(textures.isEmpty());
        assertFalse(textures.isSigned());
        assertEquals(0L, textures.getTimestamp());

        ProfileProperty property = texturesProperty(profile);
        assertNotNull(property);
        JsonObject json = decode(property);
        JsonObject skinJson = json.getAsJsonObject("textures").getAsJsonObject("SKIN");
        assertEquals(skin.toExternalForm(), skinJson.get("url").getAsString());
        assertEquals("slim", skinJson.getAsJsonObject("metadata").get("model").getAsString());

        textures.setCape(cape);
        assertEquals(cape, textures.getCape());
        json = decode(texturesProperty(profile));
        assertEquals(cape.toExternalForm(),
                json.getAsJsonObject("textures").getAsJsonObject("CAPE").get("url").getAsString());

        textures.clear();
        assertTrue(textures.isEmpty());
        assertNull(textures.getSkin());
        assertNull(textures.getCape());
        assertEquals(SkinModel.CLASSIC, textures.getSkinModel());
        assertFalse(profile.hasProperty("textures"));
    }

    @Test
    void setSkinDefaultsToClassicAndValidatesUrl() throws Exception {
        ProfileImpl profile = new ProfileImpl(ID, "Steve");
        PlayerTextures textures = profile.getTextures();
        URL skin = texture("skin");

        textures.setSkin(skin, SkinModel.SLIM);
        textures.setSkin(skin);
        assertEquals(SkinModel.CLASSIC, textures.getSkinModel());

        assertThrows(IllegalArgumentException.class,
                () -> textures.setSkin(URI.create("http://example.com/texture/skin").toURL()));
        assertThrows(IllegalArgumentException.class,
                () -> textures.setCape(URI.create("http://textures.minecraft.net/other/cape").toURL()));

        textures.setSkin(null);
        assertNull(textures.getSkin());
        assertEquals(SkinModel.CLASSIC, textures.getSkinModel());
        assertTrue(textures.isEmpty());
    }

    @Test
    void loadsTexturesPropertyConservatively() throws Exception {
        ProfileImpl profile = new ProfileImpl(ID, "Steve");
        URL skin = texture("from-property");
        URL cape = texture("cape-property");
        String value = encode("{\"timestamp\":12345,\"textures\":{"
                + "\"SKIN\":{\"url\":\"" + skin.toExternalForm()
                + "\",\"metadata\":{\"model\":\"slim\"}},"
                + "\"CAPE\":{\"url\":\"" + cape.toExternalForm() + "\"}}}");
        profile.setProperty(new ProfileProperty("textures", value, "signature"));

        PlayerTextures textures = profile.getTextures();
        assertEquals(skin, textures.getSkin());
        assertEquals(SkinModel.SLIM, textures.getSkinModel());
        assertEquals(cape, textures.getCape());
        assertEquals(12345L, textures.getTimestamp());
        assertTrue(textures.isSigned());
        // Чтение не переписывает исходное (подписанное) свойство.
        assertEquals(value, texturesProperty(profile).getValue());
        assertEquals("signature", texturesProperty(profile).getSignature());

        // Любая правка, как у Paper, делает свойство новым и неподписанным.
        textures.setSkin(skin, SkinModel.CLASSIC);
        assertEquals(SkinModel.CLASSIC, textures.getSkinModel());
        assertFalse(textures.isSigned());
        assertEquals(0L, textures.getTimestamp());
        assertNull(texturesProperty(profile).getSignature());
        assertFalse(decode(texturesProperty(profile))
                .getAsJsonObject("textures").getAsJsonObject("SKIN")
                .has("metadata"));
    }

    @Test
    void propertyReplacementIsVisibleThroughStableTexturesObject() throws Exception {
        ProfileImpl profile = new ProfileImpl(ID, "Steve");
        PlayerTextures textures = profile.getTextures();
        assertTrue(textures.isEmpty());

        URL skin = texture("replaced");
        String value = encode("{\"textures\":{\"SKIN\":{\"url\":\""
                + skin.toExternalForm() + "\"}}}");
        profile.setProperty(new ProfileProperty("textures", value));

        assertSame(textures, profile.getTextures());
        assertEquals(skin, textures.getSkin());
        assertEquals(SkinModel.CLASSIC, textures.getSkinModel());
        assertFalse(textures.isSigned());

        assertTrue(profile.removeProperty("textures"));
        assertSame(textures, profile.getTextures());
        assertTrue(textures.isEmpty());
    }

    @Test
    void setTexturesCopiesValuesAndKeepsSameIdentitySignature() throws Exception {
        URL skin = texture("source-skin");
        URL cape = texture("source-cape");
        ProfileImpl source = new ProfileImpl(ID, "Steve");
        source.setProperty(new ProfileProperty("textures", encode(
                "{\"textures\":{\"SKIN\":{\"url\":\"" + skin.toExternalForm()
                        + "\",\"metadata\":{\"model\":\"slim\"}},"
                        + "\"CAPE\":{\"url\":\"" + cape.toExternalForm() + "\"}}}"), "signature"));
        ProfileProperty sourceProperty = texturesProperty(source);

        ProfileImpl target = new ProfileImpl(ID, "Steve");
        target.setTextures(source.getTextures());
        assertEquals(skin, target.getTextures().getSkin());
        assertEquals(SkinModel.SLIM, target.getTextures().getSkinModel());
        assertEquals(cape, target.getTextures().getCape());
        assertEquals(sourceProperty, texturesProperty(target));
        assertTrue(target.getTextures().isSigned());

        // Другой профиль не должен унаследовать чужую подпись/идентичность,
        // но значения скина и плаща копируются.
        ProfileImpl other = new ProfileImpl(new UUID(3, 4), "Alex");
        other.setTextures(source.getTextures());
        assertEquals(skin, other.getTextures().getSkin());
        assertEquals(SkinModel.SLIM, other.getTextures().getSkinModel());
        assertEquals(cape, other.getTextures().getCape());
        assertNull(texturesProperty(other).getSignature());
        assertFalse(other.getTextures().isSigned());

        target.setTextures(null);
        assertTrue(target.getTextures().isEmpty());
        assertFalse(target.hasProperty("textures"));
    }

    @Test
    void malformedTexturesPropertyDoesNotThrow() {
        ProfileImpl profile = new ProfileImpl(ID, "Steve");
        profile.setProperty(new ProfileProperty("textures", "not base64 at all"));
        assertTrue(profile.getTextures().isEmpty());
        assertNull(profile.getTextures().getSkin());
        assertEquals(SkinModel.CLASSIC, profile.getTextures().getSkinModel());
        assertEquals(0L, profile.getTextures().getTimestamp());

        profile.setProperty(new ProfileProperty("textures", encode("[]")));
        assertTrue(profile.getTextures().isEmpty());

        profile.setProperty(new ProfileProperty("textures", encode(
                "{\"textures\":{\"SKIN\":{\"url\":\"http://bad host/texture/x\","
                        + "\"metadata\":{\"model\":\"unknown\"}}}}")));
        assertTrue(profile.getTextures().isEmpty());
        assertEquals(SkinModel.CLASSIC, profile.getTextures().getSkinModel());
    }

    @Test
    void cloneKeepsTexturesAndDoesNotShareChanges() throws Exception {
        URL skin = texture("clone");
        ProfileImpl profile = new ProfileImpl(ID, "Steve");
        profile.getTextures().setSkin(skin, SkinModel.SLIM);

        ProfileImpl copy = profile.clone();
        assertEquals(skin, copy.getTextures().getSkin());
        assertEquals(SkinModel.SLIM, copy.getTextures().getSkinModel());

        copy.getTextures().setCape(texture("clone-cape"));
        assertNull(profile.getTextures().getCape());
        assertNotNull(copy.getTextures().getCape());
    }
}
