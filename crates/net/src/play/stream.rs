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
    lit_tx: tokio::sync::mpsc::UnboundedSender<Vec<Frame>>,
    pub(super) lit: tokio::sync::mpsc::UnboundedReceiver<Vec<Frame>>,
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
        }
    }

    pub(super) fn center(&self) -> (i32, i32) {
        self.center
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

    async fn stream_enter(
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
        s.waiting
            .retain(|&p| (p.0 - new.0).abs() <= r && (p.1 - new.1).abs() <= r);
        let center = new;
        s.queue
            .retain(|&p| (p.0 - center.0).abs() <= r && (p.1 - center.1).abs() <= r);
        s.built
            .retain(|&p| (p.0 - center.0).abs() <= r + 1 && (p.1 - center.1).abs() <= r + 1);

        let mut cached = Vec::new();
        {
            // Порядок замков как в `commit_light`: мир, затем кадры.
            let world = self.edited.read().unwrap();
            let cache = self.chunk_frames.lock().unwrap();
            for c in fresh {
                if let Some(f) = cache.get(&c) {
                    cached.push(f.clone());
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
        if !cached.is_empty() {
            self.send_batch(conn, &cached).await?;
        }
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
        while s.in_flight < limit {
            let Some((x, z)) = s.queue.pop() else { break };
            let (plan, task_pool, tx) = (plan.clone(), pool.clone(), s.tx.clone());
            pool.submit(Box::new(move || {
                plan.generate_chunk(x, z);
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
        tokio::task::spawn_blocking(move || {
            plan.light_ring(&batch);
            let frames: Vec<Frame> = batch.iter().map(|&(x, z)| plan.chunk_frame(x, z)).collect();
            let _ = tx.send(frames);
        });
    }

    /// Пачка освещена: отправить и начать следующую.
    pub(super) async fn stream_lit(
        &self,
        conn: &mut Connection,
        s: &mut ChunkStream,
        frames: Vec<Frame>,
    ) -> Result<(), NetError> {
        s.lighting = false;
        self.stream_light(s);
        self.send_batch(conn, &frames).await
    }
}
