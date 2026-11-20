package org.bukkit.craftbukkit.help;

import java.util.LinkedHashMap;
import java.util.Map;
import org.bukkit.help.HelpTopic;

/**
 * CraftBukkit {@code SimpleHelpMap}.
 *
 * <p>CommandAPI читает поле {@code helpTopics} через VarHandle, чтобы
 * добавить/удалить темы help для своих команд.
 */
public class SimpleHelpMap {
    /** Поле, которое ищет CommandAPI (имя и тип важны). */
    private final Map<String, HelpTopic> helpTopics = new LinkedHashMap<>();

    public SimpleHelpMap() {
    }

    public Map<String, HelpTopic> helpTopicsMap() {
        return this.helpTopics;
    }

    public void clear() {
        this.helpTopics.clear();
    }
}
