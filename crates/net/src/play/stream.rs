//! Подгрузка чанков на ходу, не останавливая сессию игрока.
//!
//! Раньше переход в соседний чанк ждал отправки: генерации всей новой
//! полосы радиуса. На дальности 32 это сотни чанков, и всё это время сессия не
//! читала пакеты — команды, клики и чат игрока ждали минутами, хотя тик шёл 20.
//! Здесь переход только ставит недостающие чанки в очередь, а готовые приходят
//! в цикл сессии отдельной веткой `select!` и уходят клиенту по мере счёта:
//! ближние первыми, со светом, когда построены их соседи.

use super::*;

/// Сколько задач держать в пуле на одного игрока: пул не простаивает, но и
/// не забивается полосой, которая через секунду уже не нужна.
const IN_FLIGHT_PER_WORKER: usize = 2;

/// Чанков в одной пачке света: соседи копируются один раз на пачку, но и
/// ждать её клиенту недолго.
const LIGHT_BATCH: usize = 96;

/// `PlayerChunkSender`: освещённые чанки ждут здесь и уходят пачками раз в
/// тик, сколько разрешает клиент ответами `chunk_batch_received`.
pub(super) struct ChunkSender {
    /// Готовы и ждут отправки (`pendingChunks`): уход из обзора снимает их
    /// без `forget_level_chunk`.
    pending: HashSet<(i32, i32)>,
    /// `desiredChunksPerTick`: с 9, дальше — из ответа клиента в `0.01..=64`.
    desired: f32,
    batch_quota: f32,
    unacknowledged: i32,
    /// 1 до первого ответа клиента, потом 10.
    max_unacknowledged: i32,
}

impl Default for ChunkSender {
    fn default() -> Self {
        Self {
            pending: HashSet::new(),
            desired: 9.0,
            batch_quota: 0.0,
            unacknowledged: 0,
            max_unacknowledged: 1,
        }
    }
}

impl ChunkSender {
    /// `sendNextChunks` без самой отправки: чанки пачки этого тика, ближние
    /// к `center` первыми (`ChunkPos.distanceSquared`).
    pub(super) fn next_batch(&mut self, center: (i32, i32)) -> Vec<(i32, i32)> {
        if self.unacknowledged >= self.max_unacknowledged {
            return Vec::new();
        }
        let cap = self.desired.max(1.0);
        self.batch_quota = (self.batch_quota + self.desired).min(cap);
        if self.batch_quota < 1.0 || self.pending.is_empty() {
            return Vec::new();
        }
        let max = self.batch_quota.floor() as usize;
        let mut batch: Vec<(i32, i32)> = self.pending.iter().copied().collect();
        batch.sort_by_key(|&(x, z)| (x - center.0).pow(2) + (z - center.1).pow(2));
        batch.truncate(max);
        for p in &batch {
            self.pending.remove(p);
        }
        self.unacknowledged += 1;
        self.batch_quota -= batch.len() as f32;
        batch
    }

    /// `onChunkBatchReceivedByClient`.
    pub(super) fn received(&mut self, desired: f32) {
        self.unacknowledged = (self.unacknowledged - 1).max(0);
        self.desired = if desired.is_nan() {
            0.01
        } else {
            desired.clamp(0.01, 64.0)
        };
        if self.unacknowledged == 0 {
            self.batch_quota = 1.0;
        }
        self.max_unacknowledged = 10;
    }
}

pub(super) struct ChunkStream {
    tx: tokio::sync::mpsc::UnboundedSender<(i32, i32)>,
    pub(super) ready: tokio::sync::mpsc::UnboundedReceiver<(i32, i32)>,
    center: (i32, i32),
    radius: i32,
    /// Ещё не отданы клиенту.
    waiting: HashSet<(i32, i32)>,
    /// Не отданы в пул; ближние к центру — в конце (`pop`).
    queue: Vec<(i32, i32)>,
    /// В пуле прямо сейчас.
    in_flight: usize,
    /// Построены (сгенерированы этим потоком или уже были в мире).
    built: HashSet<(i32, i32)>,
    /// Готовы к свету, ждут конца текущей пачки.
    to_light: Vec<(i32, i32)>,
    /// Пачка сейчас светится в отдельном потоке.
    lighting: bool,
    /// Освещённые пачки: кадры уже в `chunk_frames`, сюда — их позиции.
    lit_tx: tokio::sync::mpsc::UnboundedSender<Vec<(i32, i32)>>,
    pub(super) lit: tokio::sync::mpsc::UnboundedReceiver<Vec<(i32, i32)>>,
    /// Темп отправки (`PlayerChunkSender`): переживает `restart`, как в ваниле.
    pub(super) sender: ChunkSender,
    /// Уже у клиента: в них видны сущности и приходят блоки (`isChunkTracked`).
    sent: HashSet<(i32, i32)>,
    /// Позиция игрока для дальности слежения: обновляет сессия.
    pub(super) pos: [f64; 3],
}

impl ChunkStream {
    pub(super) fn new(center: (i32, i32), radius: i32) -> Self {
        let (tx, ready) = tokio::sync::mpsc::unbounded_channel();
        let (lit_tx, lit) = tokio::sync::mpsc::unbounded_channel();
        Self {
            to_light: Vec::new(),
            lighting: false,
            lit_tx,
            lit,
            tx,
            ready,
            center,
            radius,
            waiting: HashSet::new(),
            queue: Vec::new(),
            in_flight: 0,
            built: HashSet::new(),
            sender: ChunkSender::default(),
            sent: HashSet::new(),
            pos: [0.0; 3],
        }
    }

    /// Новый мир после `respawn`: прежние задания и кадры уходят в старые
    /// каналы и пропадают.
    pub(super) fn restart(&mut self, center: (i32, i32)) {
        let mut sender = std::mem::take(&mut self.sender);
        let pos = self.pos;
        sender.pending.clear();
        *self = Self::new(center, self.radius);
        self.sender = sender;
        self.pos = pos;
    }

    pub(super) fn center(&self) -> (i32, i32) {
        self.center
    }

    /// `ChunkMap.isChunkTracked`: в обзоре и уже отправлен клиенту.
    pub(super) fn tracks(&self, chunk: (i32, i32)) -> bool {
        self.sent.contains(&chunk) && in_view(self.center, self.radius, chunk)
    }

    /// `TrackedEntity.updatePlayer`: по горизонтали не дальше
    /// `min(обзор × 16, дальность сущности)` и чанк сущности у клиента.
    pub(super) fn sees(&self, pos: [f64; 3], range: i32) -> bool {
        let (dx, dz) = (self.pos[0] - pos[0], self.pos[2] - pos[2]);
        let r = f64::from((self.radius * 16).min(range));
        dx * dx + dz * dz <= r * r
            && self.tracks(((pos[0].floor() as i32) >> 4, (pos[2].floor() as i32) >> 4))
    }

    pub(super) fn radius(&self) -> i32 {
        self.radius
    }

    pub(super) fn set_radius(&mut self, radius: i32) {
        self.radius = radius;
    }

    fn sort_queue(&mut self) {
        let c = self.center;
        self.queue
            .sort_by_key(|&(x, z)| std::cmp::Reverse((x - c.0).pow(2) + (z - c.1).pow(2)));
    }
}

impl PlayPlan {
    /// Игрок перешёл в чанк `new`: выгрузить ушедшие, отдать готовые из кэша,
    /// остальное — в очередь потока.
    pub(super) async fn stream_move(
        &self,
        conn: &mut Connection,
        s: &mut ChunkStream,
        new: (i32, i32),
    ) -> Result<(), NetError> {
        let (old, r) = (s.center, s.radius);
        let forget = chunks_around(old, r, Some((new, r)));
        let fresh = chunks_around(new, r, Some((old, r)));
        self.stream_enter(conn, s, new, &forget, fresh).await
    }

    /// Вход в мир: весь радиус через поток, ближние чанки первыми. Клиент
    /// видит землю под ногами сразу, а не после счёта всех 4225 чанков.
    pub(super) async fn stream_start(
        &self,
        conn: &mut Connection,
        s: &mut ChunkStream,
    ) -> Result<(), NetError> {
        let all = chunks_around(s.center, s.radius, None);
        self.stream_enter(conn, s, s.center, &[], all).await
    }

    pub(super) async fn stream_enter(
        &self,
        conn: &mut Connection,
        s: &mut ChunkStream,
        new: (i32, i32),
        forget: &[(i32, i32)],
        fresh: Vec<(i32, i32)>,
    ) -> Result<(), NetError> {
        let r = s.radius;
        s.center = new;
        let ids = self.ids;
        let mut prefix = Vec::new();
        for &(x, z) in forget {
            s.sent.remove(&(x, z));
            // `dropChunk`: ещё не отправленный просто снимается с очереди.
            if s.sender.pending.remove(&(x, z)) {
                continue;
            }
            prefix.extend(
                self.hub
                    .frame(&play::forget_level_chunk(ids.forget_level_chunk, x, z)),
            );
        }
        prefix.extend(self.hub.frame(&play::set_chunk_cache_center(
            ids.set_chunk_cache_center,
            new.0,
            new.1,
        )));
        conn.send_frames(&prefix).await?;
        // Ушедшие из радиуса больше не нужны: ни отправлять, ни считать.
        s.waiting.retain(|&p| in_view(new, r, p));
        let center = new;
        s.queue.retain(|&p| in_view(center, r, p));
        // Построенные — с каймой в чанк вокруг описанного квадрата `r + 1`: соседи для света.
        s.built
            .retain(|&p| (p.0 - center.0).abs() <= r + 2 && (p.1 - center.1).abs() <= r + 2);

        let mut cached = Vec::new();
        {
            // Порядок замков как в `commit_light`: мир, затем кадры.
            let world = self.dim_state().edited.read().unwrap();
            let cache = self.dim_state().chunk_frames.lock().unwrap();
            for c in fresh {
                if cache.contains_key(&c) {
                    cached.push(c);
                    continue;
                }
                s.waiting.insert(c);
                if world.contains_key(&c) {
                    s.built.insert(c);
                } else {
                    s.queue.push(c);
                }
            }
        }
        // Готовые — в очередь отправки (`markChunkPendingToSend`), уйдут по тикам.
        s.sender.pending.extend(cached);
        s.sort_queue();
        self.stream_pump(s);
        // Уже построенные (с диска или от другого игрока) могут уйти сразу.
        let have: Vec<(i32, i32)> = s
            .waiting
            .iter()
            .copied()
            .filter(|p| s.built.contains(p))
            .collect();
        self.stream_send(s, &have);
        Ok(())
    }

    /// Держит пул занятым ближними чанками очереди.
    fn stream_pump(&self, s: &mut ChunkStream) {
        let Some(pool) = self.gen_pool.get().cloned() else {
            // Без пула (тесты): прямо здесь.
            while let Some((x, z)) = s.queue.pop() {
                self.generate_chunk(x, z);
                let _ = s.tx.send((x, z));
            }
            return;
        };
        let Some(plan) = self.self_weak.get().and_then(|w| w.upgrade()) else {
            return;
        };
        let limit = crate::chunkgen::workers() * IN_FLIGHT_PER_WORKER;
        let dim = self.active_dim();
        while s.in_flight < limit {
            let Some((x, z)) = s.queue.pop() else { break };
            let (plan, task_pool, tx) = (plan.clone(), pool.clone(), s.tx.clone());
            pool.submit(Box::new(move || {
                super::with_dim(dim, || plan.generate_chunk(x, z));
                task_pool.done();
                let _ = tx.send((x, z));
            }));
            s.in_flight += 1;
        }
    }

    /// Чанк из пула готов.
    pub(super) fn stream_ready(&self, s: &mut ChunkStream, pos: (i32, i32)) {
        s.in_flight = s.in_flight.saturating_sub(1);
        s.built.insert(pos);
        self.stream_pump(s);
        let mut near = Vec::with_capacity(9);
        for dx in -1..=1 {
            for dz in -1..=1 {
                near.push((pos.0 + dx, pos.1 + dz));
            }
        }
        self.stream_send(s, &near);
    }

    /// Отдаёт из `candidates` те, что ждут и чьи соседи построены или уже не
    /// ждут (отданы раньше либо вне радиуса): свет не выходит за соседний чанк.
    fn stream_send(&self, s: &mut ChunkStream, candidates: &[(i32, i32)]) {
        let ready: Vec<(i32, i32)> = candidates
            .iter()
            .copied()
            .filter(|p| s.waiting.contains(p) && s.built.contains(p))
            .filter(|&(x, z)| {
                (-1..=1).all(|dx| {
                    (-1..=1).all(|dz| {
                        let n = (x + dx, z + dz);
                        s.built.contains(&n) || !s.waiting.contains(&n)
                    })
                })
            })
            .collect();
        for p in &ready {
            s.waiting.remove(p);
        }
        s.to_light.extend(ready);
        self.stream_light(s);
    }

    /// Свет и кодирование — вне сессии: пачка уходит в блокирующий поток, а
    /// пока она считается, готовые копятся в следующую. Крупная пачка дешевле:
    /// соседи копируются и проливаются один раз на всю.
    fn stream_light(&self, s: &mut ChunkStream) {
        if s.lighting || s.to_light.is_empty() {
            return;
        }
        let Some(plan) = self.self_weak.get().and_then(|w| w.upgrade()) else {
            return;
        };
        // Ближние первыми и не больше `LIGHT_BATCH`: огромная пачка держит
        // клиент без единого чанка, пока считается целиком.
        let c = s.center;
        s.to_light
            .sort_by_key(|&(x, z)| std::cmp::Reverse((x - c.0).pow(2) + (z - c.1).pow(2)));
        let take = s.to_light.len().min(LIGHT_BATCH);
        let batch = s.to_light.split_off(s.to_light.len() - take);
        let tx = s.lit_tx.clone();
        s.lighting = true;
        let dim = self.active_dim();
        tokio::task::spawn_blocking(move || {
            super::with_dim(dim, || {
                plan.light_ring(&batch);
                // Кадры — в кэш `chunk_frames`: отправка возьмёт их оттуда.
                for &(x, z) in &batch {
                    plan.chunk_frame(x, z);
                }
            });
            let _ = tx.send(batch);
        });
    }

    /// Пачка освещена: в очередь отправки, и начать следующую.
    pub(super) fn stream_lit(&self, s: &mut ChunkStream, lit: Vec<(i32, i32)>) {
        s.lighting = false;
        self.stream_light(s);
        // Пока пачка светилась, игрок мог уйти: вне обзора не отправляются.
        let (c, r) = (s.center, s.radius);
        s.sender
            .pending
            .extend(lit.into_iter().filter(|&p| in_view(c, r, p)));
    }

    /// Тик отправки (`PlayerChunkSender.sendNextChunks`). Кадр берётся в
    /// момент отправки: правка после света сбросила кэш, и клиент получит
    /// чанк уже с ней.
    pub(super) async fn stream_tick(
        &self,
        conn: &mut Connection,
        s: &mut ChunkStream,
    ) -> Result<bool, NetError> {
        let batch = s.sender.next_batch(s.center);
        if batch.is_empty() {
            return Ok(false);
        }
        let frames: Vec<Frame> = batch.iter().map(|&(x, z)| self.chunk_frame(x, z)).collect();
        s.sent.extend(batch);
        self.send_batch(conn, &frames).await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::ChunkSender;

    fn sender(pending: impl IntoIterator<Item = (i32, i32)>) -> ChunkSender {
        let mut s = ChunkSender::default();
        s.pending.extend(pending);
        s
    }

    fn square(r: i32) -> Vec<(i32, i32)> {
        (-r..=r)
            .flat_map(|x| (-r..=r).map(move |z| (x, z)))
            .collect()
    }

    #[test]
    fn first_batch_is_nine_then_waits_for_ack() {
        let mut s = sender(square(5));
        let first = s.next_batch((0, 0));
        assert_eq!(first.len(), 9);
        // Ближние первыми: центр и кольцо вокруг.
        assert!(first.iter().all(|&(x, z)| x * x + z * z <= 2));
        // `maxUnacknowledgedBatches` = 1 до первого ответа.
        assert!(s.next_batch((0, 0)).is_empty());
        s.received(64.0);
        // Квота после ответа — 1, плюс 64 за тик, не больше 64.
        assert_eq!(s.next_batch((0, 0)).len(), 64);
        // Ответа нет, но в полёте уже до 10 пачек.
        assert_eq!(s.next_batch((0, 0)).len(), 121 - 9 - 64);
    }

    #[test]
    fn slow_client_gets_fractional_rate() {
        let mut s = sender(square(3));
        s.next_batch((0, 0));
        s.received(0.25);
        // Квота 1 после ответа: первый тик — один чанк.
        assert_eq!(s.next_batch((0, 0)).len(), 1);
        s.received(0.25);
        // Потолок квоты `max(1, desired)` = 1, прибавка 0.25: квота
        // 1 → отправка, затем три тика копится, на четвёртый — снова чанк.
        let sent: Vec<usize> = (0..5).map(|_| s.next_batch((0, 0)).len()).collect();
        assert_eq!(sent, [1, 0, 0, 0, 1]);
    }

    #[test]
    fn nan_and_out_of_range_rates_clamp() {
        let mut s = ChunkSender::default();
        s.received(f32::NAN);
        assert_eq!(s.desired, 0.01);
        s.received(1000.0);
        assert_eq!(s.desired, 64.0);
        assert_eq!(s.unacknowledged, 0);
    }

    /// `TrackedEntity.updatePlayer`: дальность — меньшая из обзора и типа,
    /// и чанк сущности уже отправлен.
    #[test]
    fn sees_within_min_range_and_sent_chunk() {
        let mut v = super::ChunkStream::new((0, 0), 10);
        v.pos = [8.0, 64.0, 8.0];
        v.sent.extend(square(10));
        // Зомби (8 чанков = 128 блоков): 120 — видно, 130 — нет.
        assert!(v.sees([128.0, 64.0, 8.0], 128));
        assert!(!v.sees([138.0, 64.0, 8.0], 128));
        // Корова (160) при обзоре 4: потолок 64 блока.
        v.set_radius(4);
        assert!(v.sees([72.0, 64.0, 8.0], 160));
        assert!(!v.sees([73.0, 64.0, 8.0], 160));
        // В дальности, но чанк ещё не у клиента.
        v.sent.remove(&(2, 0));
        assert!(!v.sees([40.0, 64.0, 8.0], 160));
    }
}
