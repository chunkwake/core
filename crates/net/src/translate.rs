//! Перевод протокола для клиентов других версий (ViaVersion в мосте).
//!
//! Соединение клиента «чужой» версии получает сессию перевода: каждый пакет
//! после распаковки (до логики сервера) уходит в [`Translator`], и обратно
//! приходят 0..n пакетов в обе стороны — переводчик может и ответить клиенту
//! сам, и отдать серверу пакет, которого клиент не слал. Клиенты нашей версии
//! идут мимо, без накладных расходов.

use std::net::SocketAddr;
use std::sync::Arc;

/// Направление пакета.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    /// От клиента к серверу.
    Serverbound,
    /// От сервера к клиенту.
    Clientbound,
}

/// Результат перевода: пакеты по направлениям в порядке появления.
pub type Translated = Vec<(Dir, Vec<u8>)>;

/// Переводчик протокола (в Chunkwake — ViaVersion в JVM моста).
pub trait Translator: Send + Sync {
    /// Сессия для клиента версии `protocol`; `None` — версию не перевести.
    fn open(&self, protocol: i32, addr: SocketAddr) -> Option<u64>;
    /// Переводит пакет `packet`, идущий в направлении `dir`, дописывая
    /// результат в `out`. `Err` — переводчик закрыл соединение (причину
    /// клиенту он уже отправил в `out`).
    fn translate(&self, id: u64, dir: Dir, packet: &[u8], out: &mut Translated) -> Result<(), String>;
    /// Пакеты, которые переводчик отправил сам между вызовами (по таймеру).
    fn poll(&self, id: u64, out: &mut Translated) -> Result<(), String>;
    /// Сессия больше не нужна.
    fn close(&self, id: u64);
}

/// Переводчик в конфигурации сервера: замыкание не реализует `Debug`.
#[derive(Clone)]
pub struct TranslatorRef(pub Arc<dyn Translator>);

impl std::fmt::Debug for TranslatorRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Translator(..)")
    }
}

/// Сессия перевода на соединении.
pub(crate) struct Session {
    pub id: u64,
    pub translator: Arc<dyn Translator>,
    /// Версия протокола клиента.
    pub protocol: i32,
}
