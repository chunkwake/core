//! Проигрыватель (`JukeboxBlock`, `JukeboxBlockEntity`, `JukeboxSongPlayer`,
//! `JukeboxPlayable`).
//!
//! Пластинка лежит в блок-сущности (`RecordItem`), свойство `has_record`
//! блока — есть ли она. Играющая песня (`JukeboxSongPlayer`) живёт в памяти
//! измерения: тик считает время и раз в 20 тиков пускает ноту, а через
//! `lengthInTicks + 20` тиков останавливается. `ticks_since_song_started`
//! вписывается в копию столбца при сохранении ([`stamp_ticks`]), как
//! `saveAdditional` у ванили, — без правки чанка каждый тик. Пока играет — слабый сигнал 15 во все стороны;
//! компаратор читает `comparator_output` пластинки, даже когда песня кончилась.
//! Воронка кладёт пластинку в пустой проигрыватель и забирает её, если у
//! неё самой есть пустой слот.
//!
//! Нет: `setPlacedBy` с `block_entity_data` (проигрыватель с пластинкой из
//! предмета), статистики `play_record`; при смене пластинки сенсор слышит
//! `block_change`. События `jukebox_play` и `jukebox_stop_play` не передаются.

use super::*;
use chunkwake_data::world::JukeboxSong;

pub(super) const JUKEBOX: &str = "minecraft:jukebox";
/// `LevelEvent.SOUND_PLAY_JUKEBOX_SONG`.
const EVENT_PLAY: i32 = 1010;
/// `LevelEvent.SOUND_STOP_JUKEBOX_SONG`.
const EVENT_STOP: i32 = 1011;
/// `JukeboxSongPlayer.shouldEmitJukeboxPlayingEvent`: нота раз в 20 тиков.
const NOTE_PERIOD: i64 = 20;
/// `JukeboxSong.hasFinished`: песня держится ещё 20 тиков после конца.
const TAIL: i64 = 20;

/// `JukeboxSongPlayer`: песня и тики от её начала.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Playing {
    song: JukeboxSong,
    ticks: i64,
}

impl Playing {
    fn finished(&self) -> bool {
        self.ticks >= self.song.ticks + TAIL
    }
}

impl PlayPlan {
    fn song_of(&self, item: Slot) -> Option<JukeboxSong> {
        item.and_then(|s| self.survival.jukebox_songs.get(&s.item).copied())
    }

    pub(super) fn is_jukebox_disc(&self, item: Slot) -> bool {
        self.song_of(item).is_some()
    }

    fn jukebox_at(&self, pos: [i32; 3]) -> Option<u32> {
        self.block(pos).filter(|&s| self.state_name(s) == JUKEBOX)
    }

    /// Пластинка из блок-сущности (`RecordItem`).
    pub(super) fn jukebox_record(&self, pos: [i32; 3]) -> Slot {
        let be = self.block_entity(pos).filter(|b| b.id == JUKEBOX)?;
        let item = be.nbt.get("RecordItem")?.as_compound()?;
        crate::stack_nbt::from_nbt(item, |name| self.item_id(name))
    }

    /// Играет ли проигрыватель: его слабый сигнал (`ownSignal`).
    pub(super) fn jukebox_playing(&self, pos: [i32; 3]) -> bool {
        self.sim().jukeboxes.lock().unwrap().contains_key(&pos)
    }

    /// `JukeboxBlock.getAnalogOutputSignal`: `comparator_output` пластинки.
    pub(super) fn jukebox_analog(&self, pos: [i32; 3]) -> i32 {
        self.song_of(self.jukebox_record(pos)).map_or(0, |s| s.comparator)
    }

    /// `JukeboxBlockEntity.setTheItem`: пластинка в блок-сущность, `has_record`
    /// по ней, песня — играть или стоп.
    pub(super) fn jukebox_set_item(&self, pos: [i32; 3], item: Slot) {
        let Some(state) = self.jukebox_at(pos) else {
            return;
        };
        let mut be = self
            .block_entity(pos)
            .filter(|b| b.id == JUKEBOX)
            .unwrap_or_else(|| BlockEntity { id: JUKEBOX.into(), nbt: Default::default() });
        be.nbt.remove("ticks_since_song_started");
        match item.and_then(|s| crate::stack_nbt::to_nbt(s, self.item_name(s.item))) {
            Some(c) => {
                be.nbt.insert("RecordItem".into(), Tag::Compound(c));
            }
            None => {
                be.nbt.remove("RecordItem");
            }
        }
        self.set_block_entity(pos, Some(be));
        // `notifyItemChangedInJukebox`: свойство блока, если изменилось.
        let flag = if item.is_some() { "true" } else { "false" };
        if let Some(next) = self.survival.placement.with_prop(state, "has_record", flag)
            && next != state
        {
            self.set_block(pos, next);
        }
        // `JukeboxBlockEntity.notifyItemChangedInJukebox`: и при ручном
        // действии, и при переносе пластинки воронкой.
        self.emit_vibration(
            [pos[0] as f64 + 0.5, pos[1] as f64 + 0.5, pos[2] as f64 + 0.5],
            "minecraft:block_change",
        );
        match (item.is_some(), self.song_of(item)) {
            (true, Some(song)) => self.jukebox_play(pos, song),
            _ => self.jukebox_stop(pos),
        }
    }

    /// `JukeboxSongPlayer.play`: событие 1010 с номером песни.
    fn jukebox_play(&self, pos: [i32; 3], song: JukeboxSong) {
        self.sim().jukeboxes.lock().unwrap().insert(pos, Playing { song, ticks: 0 });
        self.hub.send(None, play::level_event(self.ids.level_event, EVENT_PLAY, pos, song.id, false));
        self.redstone_update(&crate::redstone::around(pos));
        self.mark_work();
    }

    /// `JukeboxSongPlayer.stop`: только если песня шла.
    fn jukebox_stop(&self, pos: [i32; 3]) {
        if self.sim().jukeboxes.lock().unwrap().remove(&pos).is_none() {
            return;
        }
        if let Some(mut be) = self.block_entity(pos).filter(|b| b.id == JUKEBOX)
            && be.nbt.remove("ticks_since_song_started").is_some()
        {
            self.set_block_entity(pos, Some(be));
        }
        self.hub.send(None, play::level_event(self.ids.level_event, EVENT_STOP, pos, 0, false));
        self.redstone_update(&crate::redstone::around(pos));
    }

    /// `popOutTheItem`: пластинка вылетает из верха блока
    /// (`atLowerCornerWithOffset(0.5, 1.01, 0.5).offsetRandomXZ(0.7)`).
    fn jukebox_pop(&self, pos: [i32; 3]) {
        let Some(record) = self.jukebox_record(pos) else {
            return;
        };
        self.jukebox_set_item(pos, None);
        self.drop_record(pos, record);
    }

    /// Пластинка над блоком: `atLowerCornerWithOffset(0.5, 1.01, 0.5)`, сдвиг
    /// по x и z до ±0.35, скорость и задержка подбора — как у `ItemEntity`.
    fn drop_record(&self, pos: [i32; 3], record: Stack) {
        let r: [f32; 2] = rand::random();
        let at = [
            pos[0] as f64 + 0.5 + ((r[0] - 0.5) * 0.7) as f64,
            pos[1] as f64 + 1.01,
            pos[2] as f64 + 0.5 + ((r[1] - 0.5) * 0.7) as f64,
        ];
        let vel = items::pop_velocity(rand::random(), rand::random());
        self.drop_item(at, vel, record, items::BLOCK_DELAY);
    }

    /// Клик по проигрывателю рукой `hand` с предметом `held`
    /// (`JukeboxBlock.useItemOn` → `useWithoutItem`). `Some(true)` — пластинка
    /// вставлена (рука тратит одну), `Some(false)` — вынута, `None` — мимо.
    pub(super) fn use_jukebox(&self, pos: [i32; 3], hand: i32, held: Slot) -> Option<bool> {
        let state = self.jukebox_at(pos)?;
        let has = self.survival.placement.prop(state, "has_record") == Some("true");
        if !has && let Some(s) = held.filter(|_| self.is_jukebox_disc(held)) {
            // `JukeboxPlayable.tryInsertIntoJukebox`.
            self.jukebox_set_item(pos, Some(Stack { count: 1, ..s }));
            return Some(true);
        }
        // `TRY_WITH_EMPTY_HAND`: пустая рука — только у основной.
        if has && hand == 0 {
            self.jukebox_pop(pos);
            return Some(false);
        }
        None
    }

    /// Блок сломан (`preRemoveSideEffects` + `setRemoved`): пластинка
    /// выпадает, клиенты глушат песню.
    pub(super) fn jukebox_removed(&self, pos: [i32; 3], old: u32) {
        if self.state_name(old) != JUKEBOX {
            return;
        }
        self.sim().jukeboxes.lock().unwrap().remove(&pos);
        if let Some(record) = self.jukebox_record(pos) {
            self.drop_record(pos, record);
        }
        self.hub.send(None, play::level_event(self.ids.level_event, EVENT_STOP, pos, 0, false));
    }

    /// Проигрыватель с диска: песня продолжается с сохранённого места
    /// (`loadAdditional` → `setSongWithoutPlaying`), событие 1010 не шлётся.
    fn adopt_jukebox(&self, pos: [i32; 3]) {
        if self.jukebox_at(pos).is_none() {
            return;
        }
        let Some(be) = self.block_entity(pos).filter(|b| b.id == JUKEBOX) else {
            return;
        };
        let Some(ticks) = be.nbt.get("ticks_since_song_started").and_then(Tag::as_i64) else {
            return;
        };
        let Some(song) = self.song_of(self.jukebox_record(pos)) else {
            return;
        };
        let p = Playing { song, ticks };
        if !p.finished() {
            self.sim().jukeboxes.lock().unwrap().insert(pos, p);
        }
    }

    /// `JukeboxBlockEntity.tick` → `JukeboxSongPlayer.tick`.
    pub(super) fn tick_jukeboxes(&self) {
        let pending = std::mem::take(&mut *self.sim().pending_jukeboxes.lock().unwrap());
        for pos in pending {
            self.adopt_jukebox(pos);
        }
        let simulated = self.simulated_set();
        let mut finished = Vec::new();
        let mut notes: Vec<([i32; 3], i64)> = Vec::new();
        {
            let mut map = self.sim().jukeboxes.lock().unwrap();
            if map.is_empty() {
                return;
            }
            for (&pos, p) in map.iter_mut() {
                if !simulated.is_empty() && !simulated.contains(&(pos[0] >> 4, pos[2] >> 4)) {
                    continue;
                }
                if p.finished() {
                    finished.push(pos);
                    continue;
                }
                if p.ticks % NOTE_PERIOD == 0 {
                    notes.push((pos, p.ticks));
                }
                p.ticks += 1;
            }
        }
        for pos in finished {
            if self.jukebox_at(pos).is_some() {
                self.jukebox_stop(pos);
            } else {
                self.sim().jukeboxes.lock().unwrap().remove(&pos);
            }
        }
        let note = self.particle_id("minecraft:note");
        for (pos, _) in notes {
            // Блок пропал без поломки (`/setblock`): песня молча кончается.
            if self.jukebox_at(pos).is_none() {
                self.sim().jukeboxes.lock().unwrap().remove(&pos);
                continue;
            }
            // `spawnMusicParticles`: `count` 0 — `xDist` задаёт цвет ноты.
            if let Some(id) = note {
                let color = rand::Rng::gen_range(&mut rand::thread_rng(), 0..4) as f32 / 24.0;
                self.hub.send(
                    None,
                    play::level_particles(
                        self.ids.level_particles,
                        id,
                        false,
                        [pos[0] as f64 + 0.5, pos[1] as f64 + 1.2, pos[2] as f64 + 0.5],
                        [color, 0.0, 0.0],
                        1.0,
                        0,
                    ),
                );
            }
        }
    }
}

/// `saveAdditional`: место в песне играющих проигрывателей — в копию
/// столбца, которая уходит на диск.
pub(super) fn stamp_ticks(column: &mut ChunkColumn, playing: &FastMap<[i32; 3], Playing>) {
    for (pos, be) in column.block_entities.iter_mut() {
        if be.id != JUKEBOX {
            continue;
        }
        if let Some(p) = playing.get(pos) {
            be.nbt.insert("ticks_since_song_started".into(), Tag::Long(p.ticks));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(ticks: i64) -> JukeboxSong {
        JukeboxSong { id: 5, comparator: 2, ticks }
    }

    /// `hasFinished`: песня держится ещё 20 тиков после `lengthInTicks`.
    #[test]
    fn song_finishes_twenty_ticks_after_its_length() {
        let p = Playing { song: song(3700), ticks: 3719 };
        assert!(!p.finished());
        assert!(Playing { ticks: 3720, ..p }.finished());
    }

    /// При сохранении место в песне попадает только к играющим.
    #[test]
    fn save_stamps_ticks_of_playing_jukeboxes_only() {
        let mut column = ChunkColumn::flat(&[], 1, 0, 15, 6, false);
        for pos in [[1, 64, 1], [2, 64, 2]] {
            column.block_entities.insert(pos, BlockEntity { id: JUKEBOX.into(), nbt: Default::default() });
        }
        let mut playing = FastMap::default();
        playing.insert([1, 64, 1], Playing { song: song(100), ticks: 42 });
        stamp_ticks(&mut column, &playing);
        let ticks = |p: [i32; 3]| column.block_entities[&p].nbt.get("ticks_since_song_started").and_then(Tag::as_i64);
        assert_eq!(ticks([1, 64, 1]), Some(42));
        assert_eq!(ticks([2, 64, 2]), None);
    }
}
