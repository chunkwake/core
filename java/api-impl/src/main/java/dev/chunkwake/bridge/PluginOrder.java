package dev.chunkwake.bridge;

import java.io.File;
import java.util.ArrayList;
import java.util.Collection;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.logging.Logger;
import org.bukkit.plugin.PluginDescriptionFile;

/**
 * Порядок загрузки плагинов, как в Paper: `depend` и найденные `softdepend`
 * загружаются раньше, `loadbefore` ставит плагин перед названными.
 * Включаются они в том же порядке, выключаются — в обратном.
 *
 * <p>Плагин без обязательной зависимости не загружается (Paper пишет
 * «Unknown/missing dependency plugins»). Круг из `softdepend` разрывается в
 * пользу плагина, чьи `depend` уже на месте; круг из `depend` — ошибка для
 * всех его участников. При прочих равных порядок — по имени файла.
 */
final class PluginOrder {
    record Candidate(File jar, PluginDescriptionFile description) {
        String name() {
            return description.getName();
        }
    }

    private PluginOrder() {}

    /**
     * @param present уже зарегистрированные плагины (тени Via): зависимость
     *     от них выполнена
     */
    static List<Candidate> sort(List<Candidate> candidates, Collection<String> present, Logger log) {
        // Имя и `provides` → плагин.
        Map<String, Candidate> byName = new HashMap<>();
        for (Candidate c : candidates) {
            byName.putIfAbsent(c.name(), c);
        }
        for (Candidate c : candidates) {
            for (String alias : c.description().getProvides()) {
                byName.putIfAbsent(alias, c);
            }
        }

        // Плагины без обязательных зависимостей выпадают; вместе с ними —
        // зависящие от них.
        Set<Candidate> alive = new LinkedHashSet<>(candidates);
        boolean changed = true;
        while (changed) {
            changed = false;
            for (Candidate c : List.copyOf(alive)) {
                List<String> missing = new ArrayList<>();
                for (String dep : c.description().getDepend()) {
                    Candidate d = byName.get(dep);
                    if (!present.contains(dep) && (d == null || !alive.contains(d))) {
                        missing.add(dep);
                    }
                }
                if (!missing.isEmpty()) {
                    log.severe("Could not load '" + c.jar().getName() + "': Unknown/missing dependency plugins: "
                            + missing + ". Please download and install these plugins to run '" + c.name() + "'.");
                    alive.remove(c);
                    changed = true;
                }
            }
        }

        // Кто должен быть загружен раньше каждого плагина.
        Map<Candidate, Set<Candidate>> hard = new HashMap<>();
        Map<Candidate, Set<Candidate>> soft = new HashMap<>();
        for (Candidate c : alive) {
            hard.put(c, resolve(c.description().getDepend(), byName, alive, c));
            soft.put(c, resolve(c.description().getSoftDepend(), byName, alive, c));
        }
        for (Candidate c : alive) {
            for (Candidate target : resolve(c.description().getLoadBefore(), byName, alive, c)) {
                soft.get(target).add(c);
            }
        }

        List<Candidate> out = new ArrayList<>(alive.size());
        Set<Candidate> done = new HashSet<>();
        List<Candidate> pending = new ArrayList<>(alive);
        while (!pending.isEmpty()) {
            Candidate next = null;
            for (Candidate c : pending) {
                if (done.containsAll(hard.get(c)) && done.containsAll(soft.get(c))) {
                    next = c;
                    break;
                }
            }
            if (next == null) {
                // Круг из `softdepend`/`loadbefore`: первый, кому хватает `depend`.
                for (Candidate c : pending) {
                    if (done.containsAll(hard.get(c))) {
                        next = c;
                        log.warning("Circular soft dependency involving " + c.name()
                                + "; loading it before " + missingNames(soft.get(c), done));
                        break;
                    }
                }
            }
            if (next == null) {
                for (Candidate c : pending) {
                    log.severe("Could not load '" + c.jar().getName() + "': circular dependency on "
                            + missingNames(hard.get(c), done));
                }
                break;
            }
            pending.remove(next);
            done.add(next);
            out.add(next);
        }
        return out;
    }

    private static Set<Candidate> resolve(List<String> names, Map<String, Candidate> byName,
            Set<Candidate> alive, Candidate self) {
        Set<Candidate> out = new LinkedHashSet<>();
        for (String n : names) {
            Candidate c = byName.get(n);
            if (c != null && c != self && alive.contains(c)) {
                out.add(c);
            }
        }
        return out;
    }

    private static List<String> missingNames(Set<Candidate> deps, Set<Candidate> done) {
        return deps.stream().filter(d -> !done.contains(d)).map(Candidate::name).toList();
    }

    /** Обязательные зависимости плагина, которых среди загруженных нет. */
    static List<String> missing(PluginDescriptionFile desc, Collection<String> loaded) {
        return desc.getDepend().stream().filter(d -> !loaded.contains(d)).toList();
    }
}
