//! Перевод протокола через ViaVersion в JVM (см. `dev.chunkwake.bridge.via`).
//!
//! Вызовы идут прямо из задачи соединения, не через поток плагинов: перевод
//! нужен на каждом пакете, и очередь к потоку плагинов сделала бы его
//! заложником медленного плагина. FFM сам подключает поток к JVM при первом
//! upcall-е. Java отдаёт результат по пакету через downcall [`emit`] в
//! коллектор этого потока — без промежуточного буфера фиксированного размера.

use super::{Slot, Table, get, up};
use chunkwake_net::translate::{Dir, Translated, Translator};
use std::cell::RefCell;
use std::net::SocketAddr;

/// Направления в `emit` (совпадают с `ViaSessions` в Java).
const SERVERBOUND: i32 = 0;
const CLIENTBOUND: i32 = 1;
/// Via закрыла соединение; данные — причина текстом.
const CLOSED: i32 = 2;

#[derive(Default)]
struct Collector {
    out: Translated,
    closed: Option<String>,
}

thread_local! {
    static COLLECTOR: RefCell<Collector> = RefCell::default();
}

/// Пакет-результат перевода из Java (живёт на время вызова).
pub(super) extern "C" fn emit(dir: i32, ptr: *const u8, len: i32) {
    let bytes = if ptr.is_null() || len <= 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(ptr, len as usize) }
    };
    COLLECTOR.with_borrow_mut(|c| match dir {
        SERVERBOUND => c.out.push((Dir::Serverbound, bytes.to_vec())),
        CLIENTBOUND => c.out.push((Dir::Clientbound, bytes.to_vec())),
        CLOSED => c.closed = Some(String::from_utf8_lossy(bytes).into_owned()),
        _ => {}
    });
}

/// ViaVersion в JVM моста как переводчик протокола.
pub struct ViaTranslator {
    table: &'static Table,
}

impl ViaTranslator {
    /// `None` — Java не поставила слоты (Via не загружена).
    pub(super) fn new(table: &'static Table) -> Option<Self> {
        [
            Slot::UpViaOpen,
            Slot::UpVia,
            Slot::UpViaPoll,
            Slot::UpViaClose,
        ]
        .iter()
        .all(|&s| get(table, s) != 0)
        .then_some(Self { table })
    }

    /// Вызов Java с коллектором этого потока; результат — в `out`.
    fn collect(&self, out: &mut Translated, call: impl FnOnce() -> i32) -> Result<(), String> {
        COLLECTOR.with_borrow_mut(|c| {
            c.out.clear();
            c.closed = None;
        });
        let status = call();
        COLLECTOR.with_borrow_mut(|c| {
            out.append(&mut c.out);
            match c.closed.take() {
                Some(reason) => Err(reason),
                None if status != 0 => Err("translation failed".into()),
                None => Ok(()),
            }
        })
    }
}

type OpenFn = extern "C" fn(i32, *const u8, i32) -> i64;
type TranslateFn = extern "C" fn(i64, i32, *const u8, i32) -> i32;
type PollFn = extern "C" fn(i64) -> i32;
type CloseFn = extern "C" fn(i64);

impl Translator for ViaTranslator {
    fn open(&self, protocol: i32, addr: SocketAddr) -> Option<u64> {
        let addr = addr.to_string();
        let open = unsafe { up::<OpenFn>(self.table, Slot::UpViaOpen) };
        let id = open(protocol, addr.as_ptr(), addr.len() as i32);
        u64::try_from(id).ok().filter(|&id| id > 0)
    }

    fn translate(
        &self,
        id: u64,
        dir: Dir,
        packet: &[u8],
        out: &mut Translated,
    ) -> Result<(), String> {
        let translate = unsafe { up::<TranslateFn>(self.table, Slot::UpVia) };
        let dir = match dir {
            Dir::Serverbound => SERVERBOUND,
            Dir::Clientbound => CLIENTBOUND,
        };
        self.collect(out, || {
            translate(id as i64, dir, packet.as_ptr(), packet.len() as i32)
        })
    }

    fn poll(&self, id: u64, out: &mut Translated) -> Result<(), String> {
        let poll = unsafe { up::<PollFn>(self.table, Slot::UpViaPoll) };
        self.collect(out, || poll(id as i64))
    }

    fn close(&self, id: u64) {
        let close = unsafe { up::<CloseFn>(self.table, Slot::UpViaClose) };
        close(id as i64);
    }
}
