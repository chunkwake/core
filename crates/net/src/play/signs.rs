//! Таблички: блок-сущность `SignBlockEntity` (`front_text`/`back_text`,
//! `is_waxed`), редактор при установке и по клику (`openTextEdit`),
//! `sign_update` от клиента (`updateSignText`), краситель, чернила,
//! светящиеся чернила и соты (`SignApplicator`). События Paper:
//! `PlayerOpenSignEvent`/`PlayerSignOpenEvent`, `UncheckedSignChangeEvent`,
//! `SignChangeEvent`.

use super::*;
use chunkwake_protocol::play::SignUpdate;

pub(super) const SIGN: &str = "minecraft:sign";
pub(super) const HANGING_SIGN: &str = "minecraft:hanging_sign";

/// `ServerGamePacketListenerImpl.MAX_SIGN_LINE_LENGTH` (`Paper.maxSignLength`, 80 code points).
const MAX_LINE: usize = 80;

/// Тип блок-сущности по блоку: `*_sign`/`*_wall_sign` — `sign`,
/// `*_hanging_sign`/`*_wall_hanging_sign` — `hanging_sign`.
pub(super) fn sign_entity(block: &str) -> Option<&'static str> {
    let b = block.strip_prefix("minecraft:")?;
    if b.ends_with("_hanging_sign") {
        Some(HANGING_SIGN)
    } else if b.ends_with("_sign") {
        Some(SIGN)
    } else {
        None
    }
}

/// `SignText` по умолчанию: четыре пустые строки, чёрный, без свечения.
fn empty_text() -> Compound {
    Compound::from([
        (
            "messages".into(),
            Tag::List((0..4).map(|_| Tag::String(String::new())).collect()),
        ),
        ("color".into(), Tag::String("black".into())),
        ("has_glowing_text".into(), Tag::Byte(0)),
    ])
}

/// NBT новой таблички (`saveAdditional`: `front_text`, `back_text`, `is_waxed`).
pub(super) fn new_sign_nbt() -> Compound {
    Compound::from([
        ("front_text".into(), Tag::Compound(empty_text())),
        ("back_text".into(), Tag::Compound(empty_text())),
        ("is_waxed".into(), Tag::Byte(0)),
    ])
}

fn side_key(front: bool) -> &'static str {
    if front { "front_text" } else { "back_text" }
}

/// Сторона таблички (нет — пустая).
pub(super) fn side(nbt: &Compound, front: bool) -> Compound {
    nbt.get(side_key(front))
        .and_then(Tag::as_compound)
        .cloned()
        .unwrap_or_else(empty_text)
}

/// Строки стороны, ровно четыре.
pub(super) fn messages(side: &Compound) -> Vec<Tag> {
    let mut out: Vec<Tag> = side
        .get("messages")
        .and_then(Tag::as_list)
        .map(|l| l.iter().take(4).cloned().collect())
        .unwrap_or_default();
    out.resize(4, Tag::String(String::new()));
    out
}

/// `Component.getString` для литералов: текст и `extra`.
fn plain(t: &Tag) -> String {
    match t {
        Tag::String(s) => s.clone(),
        Tag::List(l) => l.iter().map(plain).collect(),
        Tag::Compound(c) => {
            let mut s = c.get("text").and_then(Tag::as_str).unwrap_or_default().to_owned();
            if let Some(extra) = c.get("extra") {
                s.push_str(&plain(extra));
            }
            s
        }
        _ => String::new(),
    }
}

/// `SignText.hasMessage`: хоть одна непустая строка.
pub(super) fn has_message(side: &Compound) -> bool {
    messages(side).iter().any(|m| !plain(m).is_empty())
}

/// `SignText.hasEditableText`: все строки — простой текст без вложений
/// (клиент правит только литералы).
pub(super) fn has_editable_text(side: &Compound) -> bool {
    messages(side).iter().all(|m| match m {
        Tag::String(_) => true,
        Tag::Compound(c) => {
            !c.contains_key("extra")
                && ["translate", "score", "selector", "keybind", "nbt", "object"]
                    .iter()
                    .all(|k| !c.contains_key(*k))
        }
        _ => false,
    })
}

pub(super) fn is_waxed(nbt: &Compound) -> bool {
    matches!(nbt.get("is_waxed"), Some(Tag::Byte(b)) if *b != 0)
}

/// Строка игрока со стилем прежней (`updateMessages`: `literal(raw).setStyle(old.getStyle())`).
fn styled_like(old: &Tag, text: &str) -> Tag {
    match old {
        Tag::Compound(c) => {
            let mut c: Compound = c
                .iter()
                .filter(|(k, _)| {
                    !matches!(
                        k.as_str(),
                        "text" | "extra" | "type" | "translate" | "with" | "fallback" | "score"
                            | "selector" | "separator" | "keybind" | "nbt" | "source" | "block"
                            | "entity" | "storage" | "interpret" | "object"
                    )
                })
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            if c.is_empty() {
                return Tag::String(text.to_owned());
            }
            c.insert("text".into(), Tag::String(text.to_owned()));
            Tag::Compound(c)
        }
        _ => Tag::String(text.to_owned()),
    }
}

/// Строка из `sign_update` до `FilteredText`: обрезка до 80 code points
/// (Paper) и `ChatFormatting.stripFormatting` (`§` + код).
pub(super) fn clean_line(line: &str) -> String {
    let cut: String = line.chars().take(MAX_LINE).collect();
    let mut out = String::with_capacity(cut.len());
    let mut chars = cut.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '§'
            && let Some(&n) = chars.peek()
            && "0123456789abcdefklmnorABCDEFKLMNOR".contains(n)
        {
            chars.next();
            continue;
        }
        out.push(c);
    }
    out
}

/// `StringUtil.filterText`: без `§`, управляющих символов и DEL.
pub(super) fn filter_text(s: &str) -> String {
    s.chars()
        .filter(|&c| c != '§' && c >= ' ' && c != '\u{7f}')
        .collect()
}

/// `Mth.wrapDegrees` для `float`.
fn wrap_degrees(f: f32) -> f32 {
    let mut f = f % 360.0;
    if f >= 180.0 {
        f -= 360.0;
    }
    if f < -180.0 {
        f += 360.0;
    }
    f
}

/// `Direction.toYRot` по имени стороны.
fn facing_yaw(facing: &str) -> f32 {
    match facing {
        "south" => 0.0,
        "west" => 90.0,
        "north" => 180.0,
        _ => 270.0,
    }
}

/// `getSlotPlayerIsFacing`: игрок с лицевой стороны, если угол от центра
/// хитбокса к нему отличается от поворота таблички не больше чем на 90°.
/// `rotation` — `RotationSegment.convertToDegrees` (×22,5), иначе `facing`;
/// центр хитбокса настенной таблички — у стены (`WallSignBlock.SHAPES`).
pub(super) fn facing_front(
    pos: [i32; 3],
    rotation: Option<i32>,
    facing: Option<&str>,
    wall_plain: bool,
    player: [f64; 3],
) -> bool {
    let yaw = match (rotation, facing) {
        (Some(r), _) => r as f32 * 22.5,
        (None, Some(f)) => facing_yaw(f),
        _ => 0.0,
    };
    let center = match (wall_plain, facing) {
        (true, Some("north")) => [0.5, 0.9375],
        (true, Some("south")) => [0.5, 0.0625],
        (true, Some("west")) => [0.9375, 0.5],
        (true, Some("east")) => [0.0625, 0.5],
        _ => [0.5, 0.5],
    };
    let dx = player[0] - (pos[0] as f64 + center[0]);
    let dz = player[2] - (pos[2] as f64 + center[1]);
    let angle = (dz.atan2(dx) * 180.0 / std::f64::consts::PI) as f32 - 90.0;
    wrap_degrees(angle - yaw).abs() <= 90.0
}

/// Цвет красителя по предмету (`DataComponents.DYE` у `*_dye`).
fn dye_color(item: &str) -> Option<&'static str> {
    let color = item.strip_prefix("minecraft:")?.strip_suffix("_dye")?;
    chunkwake_protocol::components::DYE_COLORS
        .iter()
        .copied()
        .find(|&c| c == color)
}

/// Итог клика по табличке.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SignUse {
    /// Предмет применён (`SUCCESS`): списать один, если не творческий.
    Applied,
    /// Клик обработан (редактор, звук отказа) — дальше не идти.
    Handled,
    /// `PASS`: обычное использование предмета (установка блока и т. п.).
    Pass,
}

impl PlayPlan {
    /// Новая табличка без текста; клиентам — `block_entity_data`.
    pub(super) fn place_sign(&self, pos: [i32; 3], id: &str) {
        self.put_sign(pos, id, new_sign_nbt());
    }

    /// Блок-сущность таблички и `sendBlockUpdated` всем.
    fn put_sign(&self, pos: [i32; 3], id: &str, nbt: Compound) {
        self.set_block_entity(
            pos,
            Some(BlockEntity {
                id: id.into(),
                nbt: nbt.clone(),
            }),
        );
        if let Some(packet) = self.sign_packet(pos, id, &nbt) {
            self.hub.send(None, packet);
        }
    }

    fn sign_packet(&self, pos: [i32; 3], id: &str, nbt: &Compound) -> Option<Vec<u8>> {
        let kind = *chunkwake_world::BLOCK_ENTITY_TYPES.get()?.get(id)?;
        let bytes = chunkwake_world::nbt::write_network(nbt);
        Some(play::block_entity_data(self.ids.block_entity_data, pos, kind, &bytes))
    }

    /// Табличка в этом месте: тип блок-сущности и её NBT.
    pub(super) fn sign_at(&self, pos: [i32; 3]) -> Option<(&'static str, Compound)> {
        let id = sign_entity(self.state_name(self.block(pos)?))?;
        let nbt = self
            .block_entity(pos)
            .filter(|b| b.id == id)
            .map(|b| b.nbt)
            .unwrap_or_else(new_sign_nbt);
        Some((id, nbt))
    }

    /// Смотрит ли игрок на лицевую сторону таблички.
    pub(super) fn sign_front_for(&self, pos: [i32; 3], player: [f64; 3]) -> bool {
        let Some(state) = self.block(pos) else {
            return true;
        };
        let pl = &self.survival.placement;
        let name = self.state_name(state);
        let rotation = pl.prop(state, "rotation").and_then(|r| r.parse().ok());
        let wall_plain = name.ends_with("_wall_sign") && !name.ends_with("_hanging_sign");
        facing_front(pos, rotation, pl.prop(state, "facing"), wall_plain, player)
    }

    /// `isWithinBlockInteractionRange(pos, 4.0)`: дальность руки + 4.
    fn sign_in_range(&self, uuid: Uuid, pos: [i32; 3], extra: f64) -> bool {
        let creative = self.live.lock().unwrap().get(&uuid).is_some_and(|l| l.game_mode == 1);
        let reach = if creative { 5.0 } else { 4.5 };
        self.player_at(uuid)
            .is_some_and(|(p, _, _)| block_reach(eye_of(p), pos, reach + extra))
    }

    /// `getPlayerWhoMayEdit` с `clearInvalidPlayerWhoMayEdit`: ушедший или
    /// отошедший редактор забывается.
    pub(super) fn sign_editor(&self, pos: [i32; 3]) -> Option<Uuid> {
        let key = (self.active_dim(), pos);
        let mut editors = self.sign_editors.lock().unwrap();
        let id = *editors.get(&key)?;
        if self.sign_in_range(id, pos, 4.0) {
            Some(id)
        } else {
            editors.remove(&key);
            None
        }
    }

    /// `SignBlock.openTextEdit`: `PlayerOpenSignEvent` (и `PlayerSignOpenEvent`
    /// при слушателях), затем редактор у клиента и право правки.
    pub(super) async fn open_sign(&self, uuid: Uuid, pos: [i32; 3], front: bool, cause: &str) -> bool {
        let side = if front { "FRONT" } else { "BACK" };
        if self
            .cancelled(Event::OpenSign, uuid, || {
                format!("{} {} {} {side} {cause}", pos[0], pos[1], pos[2])
            })
            .await
        {
            return false;
        }
        self.open_sign_now(uuid, pos, front);
        true
    }

    /// `setAllowedPlayerEditor` и `ServerPlayer.openTextEdit`: блок и редактор клиенту.
    pub(super) fn open_sign_now(&self, uuid: Uuid, pos: [i32; 3], front: bool) {
        self.sign_editors
            .lock()
            .unwrap()
            .insert((self.active_dim(), pos), uuid);
        let state = self.block(pos).unwrap_or(AIR);
        self.send_to(uuid, play::block_update(self.ids.block_update, pos, state as i32));
        self.send_to(uuid, play::open_sign_editor(self.ids.open_sign_editor, pos, front));
    }

    /// Редактор после установки (`ItemStack.useOn`, причина `PLACE`).
    pub(super) async fn sign_placed(&self, uuid: Uuid, pos: [i32; 3]) {
        let Some((_, nbt)) = self.sign_at(pos) else {
            return;
        };
        if !is_waxed(&nbt) && has_editable_text(&side(&nbt, true)) {
            self.open_sign(uuid, pos, true, "PLACE").await;
        }
    }

    /// `SignBlock.useItemOn`, затем `useWithoutItem` (только основная рука):
    /// краситель, чернила и соты; у вощёной — звук отказа; иначе редактор
    /// той стороны, к которой стоит игрок.
    pub(super) async fn use_sign(
        &self,
        uuid: Uuid,
        pos: [i32; 3],
        held: Slot,
        main_hand: bool,
        may_build: bool,
    ) -> SignUse {
        let Some((id, mut nbt)) = self.sign_at(pos) else {
            return SignUse::Pass;
        };
        let Some((feet, _, _)) = self.player_at(uuid) else {
            return SignUse::Pass;
        };
        let front = self.sign_front_for(pos, feet);
        let other_editing = self.sign_editor(pos).is_some_and(|e| e != uuid);
        let item = held.map(|s| self.item_name(s.item).to_owned());
        let applicator = item.as_deref().filter(|n| {
            dye_color(n).is_some()
                || matches!(*n, "minecraft:ink_sac" | "minecraft:glow_ink_sac" | "minecraft:honeycomb")
        });
        if let Some(item) = applicator
            && may_build
            && !is_waxed(&nbt)
            && !other_editing
        {
            let key = side_key(front);
            let mut text = side(&nbt, front);
            let can = item == "minecraft:honeycomb" || has_message(&text);
            let applied = can
                && match item {
                    "minecraft:honeycomb" => {
                        nbt.insert("is_waxed".into(), Tag::Byte(1));
                        self.hub.send(None, play::level_event(self.ids.level_event, 3003, pos, 0, false));
                        self.block_sound(None, "minecraft:item.honeycomb.wax_on", pos, 1.0, 1.0);
                        true
                    }
                    "minecraft:glow_ink_sac" | "minecraft:ink_sac" => {
                        let glow = item == "minecraft:glow_ink_sac";
                        let was = matches!(text.get("has_glowing_text"), Some(Tag::Byte(b)) if *b != 0);
                        if was == glow {
                            false
                        } else {
                            text.insert("has_glowing_text".into(), Tag::Byte(glow as i8));
                            let sound = if glow {
                                "minecraft:item.glow_ink_sac.use"
                            } else {
                                "minecraft:item.ink_sac.use"
                            };
                            self.block_sound(None, sound, pos, 1.0, 1.0);
                            true
                        }
                    }
                    dye => {
                        let color = dye_color(dye).unwrap_or("black");
                        if text.get("color").and_then(Tag::as_str).unwrap_or("black") == color {
                            false
                        } else {
                            text.insert("color".into(), Tag::String(color.into()));
                            self.block_sound(None, "minecraft:item.dye.use", pos, 1.0, 1.0);
                            true
                        }
                    }
                };
            if applied {
                if item != "minecraft:honeycomb" {
                    nbt.insert(key.into(), Tag::Compound(text));
                }
                self.put_sign(pos, id, nbt);
                // `GameEvent.BLOCK_CHANGE` от игрока.
                self.emit_vibration(
                    [pos[0] as f64 + 0.5, pos[1] as f64 + 0.5, pos[2] as f64 + 0.5],
                    "minecraft:block_change",
                );
                return SignUse::Applied;
            }
        }
        // `TRY_WITH_EMPTY_HAND` → `useWithoutItem` лишь для основной руки.
        if !main_hand {
            return SignUse::Pass;
        }
        if is_waxed(&nbt) {
            let sound = if id == HANGING_SIGN {
                "minecraft:block.hanging_sign.waxed_interact_fail"
            } else {
                "minecraft:block.sign.waxed_interact_fail"
            };
            self.block_sound(None, sound, pos, 1.0, 1.0);
            return SignUse::Handled;
        }
        if !other_editing && may_build && has_editable_text(&side(&nbt, front)) {
            self.open_sign(uuid, pos, front, "INTERACT").await;
            return SignUse::Handled;
        }
        SignUse::Pass
    }

    /// `handleSignUpdate` → `updateSignText`: Paper режет строки до 80 code
    /// points и снимает коды `§`, зовёт `UncheckedSignChangeEvent`; правит только
    /// игрок, которому открыт редактор, у невощёной таблички рядом. Строки
    /// сохраняют стиль прежних, `SignChangeEvent` может их сменить или отменить.
    pub(super) async fn sign_update(&self, uuid: Uuid, u: SignUpdate) {
        let pos = u.pos;
        let raw: Vec<String> = u.lines.iter().map(|l| clean_line(l)).collect();
        let side_name = if u.front { "FRONT" } else { "BACK" };
        let head = format!("{} {} {} {side_name}", pos[0], pos[1], pos[2]);
        if self
            .cancelled(Event::SignUnchecked, uuid, || {
                let lines: Vec<String> = raw
                    .iter()
                    .map(|l| serde_json::Value::String(l.clone()).to_string())
                    .collect();
                format!("{head}\n{}", lines.join("\n"))
            })
            .await
        {
            return;
        }
        let Some((id, mut nbt)) = self.sign_at(pos) else {
            return;
        };
        let editor = self.sign_editor(pos);
        if is_waxed(&nbt) || editor != Some(uuid) {
            tracing::warn!("Player {uuid} just tried to change non-editable sign");
            // Рядом (до 32 блоков) — вернуть клиенту настоящий текст.
            if let Some((feet, _, _)) = self.player_at(uuid) {
                let d = [
                    feet[0] - pos[0] as f64,
                    feet[1] - pos[1] as f64,
                    feet[2] - pos[2] as f64,
                ];
                if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < 32.0 * 32.0
                    && let Some(p) = self.sign_packet(pos, id, &nbt)
                {
                    self.send_to(uuid, p);
                }
            }
            return;
        }
        let key = side_key(u.front);
        let mut text = side(&nbt, u.front);
        let old = messages(&text);
        let mut lines: Vec<Tag> = old.iter().zip(&raw).map(|(o, r)| styled_like(o, r)).collect();
        let mut filtered: Vec<Tag> = old
            .iter()
            .zip(&raw)
            .map(|(o, r)| styled_like(o, &filter_text(r)))
            .collect();
        let reply = self
            .plugin_event(Event::SignChange, uuid, || {
                let json: Vec<String> = lines
                    .iter()
                    .map(|l| crate::stack_nbt::tag_to_json(l).to_string())
                    .collect();
                format!("{head}\n{}", json.join("\n"))
            })
            .await;
        let cancelled = reply.as_deref().is_some_and(|r| r.trim() == "c");
        if !cancelled {
            // Ответ плагина: четыре строки JSON (пустой ответ — без изменений).
            if let Some(r) = reply.as_deref().filter(|r| !r.trim().is_empty()) {
                for (i, l) in r.split('\n').take(4).enumerate() {
                    // `Mutable.setLine(i, c)`: и строка, и её фильтрованная копия.
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
                        let t = crate::stack_nbt::json_to_tag(&v);
                        if t != lines[i] {
                            filtered[i] = t.clone();
                            lines[i] = t;
                        }
                    }
                }
            }
            if filtered != lines {
                text.insert("filtered_messages".into(), Tag::List(filtered));
            } else {
                text.remove("filtered_messages");
            }
            text.insert("messages".into(), Tag::List(lines));
            nbt.insert(key.into(), Tag::Compound(text));
        }
        self.sign_editors.lock().unwrap().remove(&(self.active_dim(), pos));
        self.put_sign(pos, id, nbt);
    }

    /// Запрос плагина `sign x y z` → JSON таблички (пусто — не табличка);
    /// `sign x y z\n<json>` — записать и разослать (`Sign.update`).
    pub(super) fn sign_request(&self, rest: &str) -> String {
        let (head, body) = rest.split_once('\n').unwrap_or((rest, ""));
        let mut it = head.split_whitespace().filter_map(|v| v.parse::<i32>().ok());
        let (Some(x), Some(y), Some(z)) = (it.next(), it.next(), it.next()) else {
            return String::new();
        };
        let pos = [x, y, z];
        let Some((id, mut nbt)) = self.sign_at(pos) else {
            return String::new();
        };
        // `sign x y z side px pz` → сторона, к которой обращена точка
        // (`Sign.getInteractableSideFor`).
        if let Some((_, at)) = head.split_once(" side ") {
            let mut p = at.split_whitespace().filter_map(|v| v.parse::<f64>().ok());
            let (Some(px), Some(pz)) = (p.next(), p.next()) else {
                return String::new();
            };
            let front = self.sign_front_for(pos, [px, pos[1] as f64, pz]);
            return if front { "FRONT" } else { "BACK" }.into();
        }
        let side_json = |front: bool| {
            let s = side(&nbt, front);
            serde_json::json!({
                "color": s.get("color").and_then(Tag::as_str).unwrap_or("black"),
                "glowing": matches!(s.get("has_glowing_text"), Some(Tag::Byte(b)) if *b != 0),
                "lines": messages(&s).iter().map(crate::stack_nbt::tag_to_json).collect::<Vec<_>>(),
            })
        };
        if body.is_empty() {
            return serde_json::json!({
                "hanging": id == HANGING_SIGN,
                "waxed": is_waxed(&nbt),
                "editor": self.sign_editor(pos).map(|u| u.to_string()),
                "front": side_json(true),
                "back": side_json(false),
            })
            .to_string();
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
            return "0".into();
        };
        if let Some(w) = v.get("waxed").and_then(|w| w.as_bool()) {
            nbt.insert("is_waxed".into(), Tag::Byte(w as i8));
        }
        for front in [true, false] {
            let Some(sv) = v.get(if front { "front" } else { "back" }) else {
                continue;
            };
            let mut s = side(&nbt, front);
            if let Some(c) = sv.get("color").and_then(|c| c.as_str()) {
                s.insert("color".into(), Tag::String(c.into()));
            }
            if let Some(g) = sv.get("glowing").and_then(|g| g.as_bool()) {
                s.insert("has_glowing_text".into(), Tag::Byte(g as i8));
            }
            if let Some(l) = sv.get("lines").and_then(|l| l.as_array()) {
                let mut lines: Vec<Tag> = l.iter().take(4).map(crate::stack_nbt::json_to_tag).collect();
                lines.resize(4, Tag::String(String::new()));
                s.insert("messages".into(), Tag::List(lines));
                s.remove("filtered_messages");
            }
            nbt.insert(side_key(front).into(), Tag::Compound(s));
        }
        self.put_sign(pos, id, nbt);
        "1".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_by_block_name() {
        assert_eq!(sign_entity("minecraft:oak_sign"), Some(SIGN));
        assert_eq!(sign_entity("minecraft:spruce_wall_sign"), Some(SIGN));
        assert_eq!(sign_entity("minecraft:cherry_hanging_sign"), Some(HANGING_SIGN));
        assert_eq!(sign_entity("minecraft:bamboo_wall_hanging_sign"), Some(HANGING_SIGN));
        assert_eq!(sign_entity("minecraft:oak_planks"), None);
    }

    #[test]
    fn lines_are_cut_and_stripped_like_paper() {
        assert_eq!(clean_line("§cRed §lbold"), "Red bold");
        assert_eq!(clean_line(&"x".repeat(100)).chars().count(), 80);
        // Не код форматирования: `§` остаётся в raw, но не в filtered.
        assert_eq!(clean_line("a§zb"), "a§zb");
        assert_eq!(filter_text("a§zb\u{7}"), "azb");
    }

    #[test]
    fn facing_side_follows_rotation_and_wall() {
        // Стоячая табличка `rotation=0` смотрит на юг (+z): игрок южнее — лицо.
        assert!(facing_front([0, 64, 0], Some(0), None, false, [0.5, 64.0, 3.0]));
        assert!(!facing_front([0, 64, 0], Some(0), None, false, [0.5, 64.0, -3.0]));
        // `rotation=8` (180°) — наоборот.
        assert!(facing_front([0, 64, 0], Some(8), None, false, [0.5, 64.0, -3.0]));
        // Настенная `facing=north` у южной стены: игрок севернее — лицо.
        assert!(facing_front([0, 64, 0], None, Some("north"), true, [0.5, 64.0, -2.0]));
        // Стоя в той же клетке перед доской (z < 0,9375) — тоже лицо.
        assert!(facing_front([0, 64, 0], None, Some("north"), true, [0.5, 64.0, 0.5]));
        assert!(facing_front([0, 64, 0], None, Some("east"), true, [3.0, 64.0, 0.5]));
    }

    #[test]
    fn editable_text_and_styles() {
        let mut text = empty_text();
        assert!(has_editable_text(&text));
        assert!(!has_message(&text));
        let red = Tag::Compound(Compound::from([
            ("text".into(), Tag::String("old".into())),
            ("color".into(), Tag::String("red".into())),
        ]));
        text.insert(
            "messages".into(),
            Tag::List(vec![red.clone(), Tag::String(String::new()), Tag::String(String::new()), Tag::String(String::new())]),
        );
        assert!(has_message(&text));
        assert_eq!(
            styled_like(&red, "new"),
            Tag::Compound(Compound::from([
                ("text".into(), Tag::String("new".into())),
                ("color".into(), Tag::String("red".into())),
            ]))
        );
        assert_eq!(styled_like(&Tag::String("x".into()), "y"), Tag::String("y".into()));
        let tr = Tag::Compound(Compound::from([("translate".into(), Tag::String("a.b".into()))]));
        text.insert("messages".into(), Tag::List(vec![tr]));
        assert!(!has_editable_text(&text));
    }
}
