package dev.chunkwake.bridge;

import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import org.junit.jupiter.api.Test;

/** {@code VanillaArgumentProvider.player/players/playerProfiles} в Chunkwake. */
final class CommandArgumentsTest {
    @Test
    void parsesSelectorsAndPlayerNames() throws Exception {
        assertNotNull(CommandArguments.player().parse(new StringReader("@a[limit=2]")));
        assertNotNull(CommandArguments.players().parse(new StringReader("@p")));
        assertNotNull(CommandArguments.player().parse(new StringReader("Notch")));
        assertNotNull(CommandArguments.playerProfiles().parse(new StringReader("@a")));
        assertNotNull(CommandArguments.playerProfiles().parse(new StringReader("Notch")));
    }

    @Test
    void parsesRotationAndResources() throws Exception {
        var rotation = CommandArguments.rotation().parse(new StringReader("90 -45"));
        var resolved = rotation.resolve(null);
        org.junit.jupiter.api.Assertions.assertEquals(90.0F, resolved.yaw());
        org.junit.jupiter.api.Assertions.assertEquals(-45.0F, resolved.pitch());

    }

    @Test
    void rejectsEmptyInput() {
        assertThrows(CommandSyntaxException.class,
                () -> CommandArguments.player().parse(new StringReader("")));
        assertThrows(CommandSyntaxException.class,
                () -> CommandArguments.playerProfiles().parse(new StringReader("")));
    }
}
