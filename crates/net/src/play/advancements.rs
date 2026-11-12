//! Достижения (`Advancement`): загрузка данных, прогресс, `update_advancements`
//! и триггеры (`villager_trade`, `fishing_rod_hooked`, `consume_item`).
//!
//! Определения читаются из `data/minecraft/advancement/**/*.json` при старте.
//! Прогресс хранится по игроку (advancement → критерий → время получения) и
//! пишется в `advancements/<uuid>.json`.

use super::*;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct AdvancementCriterion {
    pub name: String,
    pub trigger: String,
    /// `conditions.item.items`: какие предметы закрывают критерий.
    pub items: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct AdvancementDisplay {
    pub title: serde_json::Value,
    pub description: serde_json::Value,
    pub icon: String,
    /// `AdvancementType.id`: task=0, challenge=1, goal=2.
    pub frame: i32,
    pub background: Option<String>,
    pub show_toast: bool,
    pub announce: bool,
    pub hidden: bool,
}

#[derive(Clone, Debug)]
pub struct AdvancementDef {
    pub id: String,
    pub parent: Option<String>,
    pub display: Option<AdvancementDisplay>,
    pub requirements: Vec<Vec<String>>,
    pub criteria: Vec<AdvancementCriterion>,
    pub x: f32,
    pub y: f32,
}

fn short(id: &str) -> &str {
    id.trim_start_matches("minecraft:")
}

fn json_string_list(v: &serde_json::Value) -> Vec<String> {
    match v {
        serde_json::Value::String(s) => vec![s.clone()],
        serde_json::Value::Array(a) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_display(v: &serde_json::Value) -> Option<AdvancementDisplay> {
    let d = v.get("display")?;
    let icon = d
        .get("icon")
        .and_then(|i| i.get("id").or_else(|| i.get("item")))
        .and_then(|i| i.as_str())?
        .to_owned();
    let frame = match d.get("frame").and_then(|f| f.as_str()) {
        Some("challenge") => 1,
        Some("goal") => 2,
        _ => 0,
    };
    Some(AdvancementDisplay {
        title: d
            .get("title")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({"text": ""})),
        description: d
            .get("description")
            .cloned()
            .unwrap_or(serde_json::Value::String(String::new())),
        icon,
        frame,
        background: d
            .get("background")
            .and_then(|b| b.as_str())
            .map(str::to_owned),
        show_toast: d
            .get("show_toast")
            .and_then(|b| b.as_bool())
            .unwrap_or(true),
        announce: d
            .get("announce_to_chat")
            .and_then(|b| b.as_bool())
            .unwrap_or(true),
        hidden: d.get("hidden").and_then(|b| b.as_bool()).unwrap_or(false),
    })
}

fn parse_advancement(id: &str, json: &serde_json::Value) -> Option<AdvancementDef> {
    let parent = json
        .get("parent")
        .and_then(|p| p.as_str())
        .map(short)
        .map(str::to_owned);
    let display = parse_display(json);
    let mut criteria = Vec::new();
    if let Some(map) = json.get("criteria").and_then(|c| c.as_object()) {
        for (name, c) in map {
            let trigger = c
                .get("trigger")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_owned();
            let items = c
                .get("conditions")
                .and_then(|c| c.get("item"))
                .and_then(|i| i.get("items"))
                .map(json_string_list)
                .unwrap_or_default();
            criteria.push(AdvancementCriterion {
                name: name.clone(),
                trigger,
                items,
            });
        }
    }
    let requirements = json
        .get("requirements")
        .and_then(|r| r.as_array())
        .map(|groups| groups.iter().map(json_string_list).collect::<Vec<_>>())
        .filter(|g| !g.is_empty())
        .unwrap_or_else(|| vec![criteria.iter().map(|c| c.name.clone()).collect()]);
    Some(AdvancementDef {
        id: id.to_owned(),
        parent,
        display,
        requirements,
        criteria,
        x: 0.0,
        y: 0.0,
    })
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, serde_json::Value)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, out);
        } else if path.extension().is_some_and(|e| e == "json")
            && let Ok(bytes) = std::fs::read(&path)
            && let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes)
        {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, json));
        }
    }
}

/// Прочитать все достижения из `data/minecraft/advancement`.
pub fn load_advancements(data_dir: &Path) -> Vec<AdvancementDef> {
    let mut raw = Vec::new();
    let root = data_dir.join("data/minecraft/advancement");
    walk(&root, &root, &mut raw);
    raw.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out: Vec<AdvancementDef> = raw
        .iter()
        .filter_map(|(id, json)| parse_advancement(id, json))
        .collect();
    // Раскладка дерева: корни в (0,0), дети справа со сдвигом по вертикали.
    let children: FastMap<String, Vec<String>> = {
        let mut map: FastMap<String, Vec<String>> = FastMap::default();
        for a in &out {
            if let Some(p) = &a.parent {
                map.entry(p.clone()).or_default().push(a.id.clone());
            }
        }
        map
    };
    let mut pos: FastMap<String, (f32, f32)> = FastMap::default();
    let roots: Vec<String> = out
        .iter()
        .filter(|a| a.parent.is_none())
        .map(|a| a.id.clone())
        .collect();
    let mut stack: Vec<(String, f32, f32, f32)> = roots
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), 0.0, i as f32 * 2.0, 1.0))
        .collect();
    while let Some((id, x, y, _)) = stack.pop() {
        pos.insert(id.clone(), (x, y));
        if let Some(kids) = children.get(&id) {
            let n = kids.len() as f32;
            for (i, kid) in kids.iter().enumerate() {
                stack.push((
                    kid.clone(),
                    x + 1.5,
                    y + (i as f32 - (n - 1.0) / 2.0) * 1.5,
                    0.0,
                ));
            }
        }
    }
    for a in &mut out {
        if let Some(&(x, y)) = pos.get(&a.id) {
            a.x = x;
            a.y = y;
        }
    }
    out
}

impl PlayPlan {
    fn adv(&self, id: &str) -> Option<&AdvancementDef> {
        self.survival
            .advancement_ids
            .get(id)
            .and_then(|&i| self.survival.advancements.get(i))
    }

    pub(crate) fn advancement_index(&self, id: &str) -> Option<usize> {
        self.survival.advancement_ids.get(id).copied()
    }

    fn adv_done(progress: &FastMap<String, i64>, def: &AdvancementDef) -> bool {
        !def.requirements.is_empty()
            && def.requirements.iter().all(|group| {
                group
                    .iter()
                    .any(|name| progress.get(name).is_some_and(|&at| at > 0))
            })
    }

    fn adv_parent_done(&self, progress: &AdvProgress, parent: Option<&str>) -> bool {
        let Some(parent) = parent else {
            return true;
        };
        self.adv(parent).is_some_and(|def| {
            progress.get(parent).is_some_and(|p| Self::adv_done(p, def)) || def.criteria.is_empty()
        })
    }

    /// Добавить критерии по триггеру и отправить прогресс.
    pub(crate) fn advancement_trigger(&self, uuid: Uuid, trigger: &str, item: Option<&str>) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        let mut changed = Vec::new();
        let mut completed = Vec::new();
        {
            let mut all = self.advancement_progress.lock().unwrap();
            let progress = all.entry(uuid).or_default();
            for def in &self.survival.advancements {
                if !self.adv_parent_done(progress, def.parent.as_deref()) {
                    continue;
                }
                let was = progress
                    .get(&def.id)
                    .is_some_and(|p| Self::adv_done(p, def));
                let mut touched = false;
                {
                    let entry = progress.entry(def.id.clone()).or_default();
                    for c in &def.criteria {
                        if c.trigger != trigger {
                            continue;
                        }
                        if !c.items.is_empty()
                            && !item
                                .is_some_and(|name| c.items.iter().any(|n| short(n) == short(name)))
                        {
                            continue;
                        }
                        if !entry.contains_key(&c.name) {
                            entry.insert(c.name.clone(), now);
                            touched = true;
                        }
                    }
                }
                if touched {
                    changed.push(def.id.clone());
                    let entry = &progress[&def.id];
                    if !was && Self::adv_done(entry, def) {
                        completed.push(def.id.clone());
                    }
                }
            }
        }
        if !changed.is_empty() {
            self.send_advancement_progress(uuid, &changed);
        }
        for id in completed {
            self.announce_advancement(uuid, &id);
        }
    }

    fn announce_advancement(&self, uuid: Uuid, id: &str) {
        let Some(def) = self.adv(id) else {
            return;
        };
        let Some(display) = &def.display else {
            return;
        };
        if !display.announce || display.hidden {
            return;
        }
        let name = self
            .hub
            .players
            .lock()
            .unwrap()
            .get(&uuid)
            .map(|p| p.entry.name.clone())
            .unwrap_or_default();
        let kind = match display.frame {
            1 => "challenge",
            2 => "goal",
            _ => "task",
        };
        let message = serde_json::json!({
            "translate": format!("chat.type.advancement.{kind}"),
            "with": [{"text": name}, display.title],
        });
        self.hub.send(
            None,
            play::system_chat(self.ids.system_chat, &message.to_string(), false),
        );
    }

    fn adv_body(w: &mut Writer, def: &AdvancementDef, icon_id: Option<i32>) {
        // parent: optional identifier.
        w.bool(def.parent.is_some());
        if let Some(parent) = &def.parent {
            w.string(&format!("minecraft:{parent}"));
        }
        // display: optional.
        w.bool(def.display.is_some());
        if let Some(d) = &def.display {
            text::write_component(w, &d.title);
            text::write_component(w, &d.description);
            // icon: ItemStackTemplate (item, count, пустой patch).
            w.var_int(icon_id.unwrap_or(0))
                .var_int(1)
                .var_int(0)
                .var_int(0);
            w.var_int(d.frame);
            let mut flags = 0i32;
            if d.background.is_some() {
                flags |= 1;
            }
            if d.show_toast {
                flags |= 2;
            }
            if d.hidden {
                flags |= 4;
            }
            w.i32(flags);
            if let Some(bg) = &d.background {
                w.string(bg);
            }
        }
        // requirements: list<list<string>>.
        w.var_int(def.requirements.len() as i32);
        for group in &def.requirements {
            w.var_int(group.len() as i32);
            for name in group {
                w.string(name);
            }
        }
        w.bool(true);
    }

    fn advancement_packets(
        &self,
        reset: bool,
        added: &[&AdvancementDef],
        progress: &[(String, FastMap<String, i64>)],
    ) -> Vec<u8> {
        let mut w = Writer::new();
        w.var_int(self.ids.update_advancements).bool(reset);
        w.var_int(added.len() as i32);
        for def in added {
            w.string(&format!("minecraft:{}", def.id));
            let icon = def.display.as_ref().and_then(|d| self.item_id(&d.icon));
            Self::adv_body(&mut w, def, icon);
            w.f32(def.x).f32(def.y);
        }
        w.var_int(0); // removed
        w.var_int(progress.len() as i32);
        for (id, criteria) in progress {
            w.string(&format!("minecraft:{id}"));
            w.var_int(criteria.len() as i32);
            for (name, at) in criteria {
                w.string(name).bool(*at > 0).i64(*at);
            }
        }
        w.bool(true);
        w.buf
    }

    /// Полный пакет при входе: все определения и накопленный прогресс.
    pub(crate) fn full_advancement_packet(&self, uuid: Uuid) -> Option<Vec<u8>> {
        if self.survival.advancements.is_empty() {
            return None;
        }
        let all: Vec<&AdvancementDef> = self.survival.advancements.iter().collect();
        let progress = self
            .advancement_progress
            .lock()
            .unwrap()
            .get(&uuid)
            .map(|p| {
                p.iter()
                    .map(|(id, c)| (id.clone(), c.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Some(self.advancement_packets(true, &all, &progress))
    }

    fn send_advancement_progress(&self, uuid: Uuid, ids: &[String]) {
        if ids.is_empty() {
            return;
        }
        let all = self.advancement_progress.lock().unwrap();
        let progress: Vec<(String, FastMap<String, i64>)> = ids
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    all.get(&uuid)
                        .and_then(|p| p.get(id))
                        .cloned()
                        .unwrap_or_default(),
                )
            })
            .collect();
        drop(all);
        let packet = self.advancement_packets(false, &[], &progress);
        self.send_to(uuid, packet);
    }

    /// Загруженный с диска прогресс.
    pub(crate) fn load_advancement_progress(
        &self,
        uuid: Uuid,
        data: Vec<(String, Vec<(String, i64)>)>,
    ) {
        let map: AdvProgress = data
            .into_iter()
            .map(|(id, criteria)| (short(&id).to_owned(), criteria.into_iter().collect()))
            .collect();
        self.advancement_progress.lock().unwrap().insert(uuid, map);
    }

    /// Прогресс для `advancements/<uuid>.json`.
    pub(crate) fn collect_advancement_progress(
        &self,
        uuid: Uuid,
    ) -> Vec<(String, Vec<(String, i64)>)> {
        self.advancement_progress
            .lock()
            .unwrap()
            .get(&uuid)
            .map(|p| {
                p.iter()
                    .filter(|(_, c)| !c.is_empty())
                    .map(|(id, c)| (id.clone(), c.iter().map(|(k, &v)| (k.clone(), v)).collect()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn forget_advancements(&self, uuid: Uuid) {
        self.advancement_progress.lock().unwrap().remove(&uuid);
    }

    /// `/advancement grant|revoke`: список id для `everything` и потомков.
    pub(crate) fn advancement_ids(&self) -> Vec<String> {
        self.survival
            .advancements
            .iter()
            .map(|a| a.id.clone())
            .collect()
    }

    fn adv_descendants(&self, root: &str) -> Vec<String> {
        let mut out = vec![root.to_owned()];
        let mut i = 0;
        while i < out.len() {
            let parent = out[i].clone();
            for def in &self.survival.advancements {
                if def.parent.as_deref() == Some(parent.as_str()) {
                    out.push(def.id.clone());
                }
            }
            i += 1;
        }
        out
    }

    fn adv_ancestors(&self, id: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = self.adv(id).and_then(|d| d.parent.clone());
        while let Some(p) = cur {
            cur = self.adv(&p).and_then(|d| d.parent.clone());
            out.push(p);
        }
        out
    }

    /// Выдать критерии достижения (`criterion = None` — все).
    pub(crate) fn advancement_grant(&self, uuid: Uuid, id: &str, criterion: Option<&str>) {
        let Some(def) = self.adv(id) else {
            return;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        let mut changed = vec![id.to_owned()];
        {
            let mut all = self.advancement_progress.lock().unwrap();
            let entry = all
                .entry(uuid)
                .or_default()
                .entry(id.to_owned())
                .or_default();
            let names: Vec<String> = match criterion {
                Some(name) => def
                    .criteria
                    .iter()
                    .filter(|c| c.name == name)
                    .map(|c| c.name.clone())
                    .collect(),
                None => def.criteria.iter().map(|c| c.name.clone()).collect(),
            };
            for name in names {
                entry.entry(name).or_insert(now);
            }
        }
        // Родительские достижения тоже нужны для видимости.
        for p in self.adv_ancestors(id) {
            changed.push(p.clone());
        }
        self.send_advancement_progress(uuid, &changed);
    }

    pub(crate) fn advancement_revoke(&self, uuid: Uuid, id: &str, criterion: Option<&str>) {
        let mut changed = vec![id.to_owned()];
        {
            let mut all = self.advancement_progress.lock().unwrap();
            let progress = all.entry(uuid).or_default();
            match criterion {
                Some(name) => {
                    if let Some(entry) = progress.get_mut(id) {
                        entry.remove(name);
                        if entry.is_empty() {
                            progress.remove(id);
                        }
                    }
                }
                None => {
                    progress.remove(id);
                }
            }
        }
        for p in self.adv_ancestors(id) {
            changed.push(p);
        }
        self.send_advancement_progress(uuid, &changed);
    }

    /// `everything`: id всех достижений.
    pub(crate) fn advancement_all(&self) -> Vec<String> {
        self.advancement_ids()
    }

    /// `from`: достижение и потомки; `through`/`until`: предки.
    pub(crate) fn advancement_from(&self, id: &str) -> Vec<String> {
        self.adv_descendants(id)
    }

    pub(crate) fn advancement_through(&self, id: &str, include: bool) -> Vec<String> {
        let mut out = self.adv_ancestors(id);
        if include {
            out.insert(0, id.to_owned());
        }
        out
    }
}
