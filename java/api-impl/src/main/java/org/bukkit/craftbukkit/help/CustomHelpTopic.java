package org.bukkit.craftbukkit.help;

import org.bukkit.command.CommandSender;
import org.bukkit.help.HelpTopic;

/** CraftBukkit {@code CustomHelpTopic}: тема help, видимая по permission. */
public class CustomHelpTopic extends HelpTopic {
    private final String permission;

    public CustomHelpTopic(String name, String shortText, String fullText) {
        this(name, shortText, fullText, null);
    }

    public CustomHelpTopic(String name, String shortText, String fullText, String permission) {
        this.name = name;
        this.shortText = shortText;
        this.fullText = fullText;
        this.permission = permission;
    }

    @Override
    public boolean canSee(CommandSender sender) {
        if (this.permission == null || this.permission.isEmpty()) {
            return true;
        }
        return sender.hasPermission(this.permission);
    }
}
