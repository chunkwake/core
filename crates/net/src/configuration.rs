//! Фаза configuration: known packs → реестры → теги → finish.
//!
//! Все исходящие пакеты собираются один раз при старте ([`ConfigurationPlan::build`]),
//! на каждого игрока отправляются готовые байты.

use crate::{Connection, NetError};
use chunkwake_data::DataError;
use chunkwake_data::packets::{Direction, PacketTable, State};
use chunkwake_data::registries::RegistryData;
use chunkwake_protocol::Reader;
use chunkwake_protocol::configuration::{self as cfg, KnownPack};

#[derive(Debug, Clone, Copy)]
struct Ids {
    disconnect: i32,
    sb_select_known_packs: i32,
    sb_finish: i32,
    sb_client_information: i32,
    sb_custom_payload: i32,
}

#[derive(Debug)]
pub struct ConfigurationPlan {
    ids: Ids,
    pack: KnownPack,
    /// update_enabled_features + select_known_packs.
    greeting: Vec<Vec<u8>>,
    /// registry_data × N, update_tags, finish_configuration.
    sync: Vec<Vec<u8>>,
}

pub enum Outcome {
    /// Клиент подтвердил finish_configuration и перешёл в play; настройки —
    /// последний `client_information` (без него — `ClientInformation.createDefault`).
    /// Настройки клиента и каналы плагинов из `minecraft:register`.
    Finished(cfg::ClientInfo, Vec<String>),
    /// Клиент отключён с сообщением.
    Rejected,
}

impl ConfigurationPlan {
    pub fn build(
        packets: &PacketTable,
        registries: &RegistryData,
        version: &str,
    ) -> Result<Self, DataError> {
        let cb = |name| packets.id(State::Configuration, Direction::Clientbound, name);
        let sb = |name| packets.id(State::Configuration, Direction::Serverbound, name);
        let pack = KnownPack::vanilla(version);

        let greeting = vec![
            cfg::update_enabled_features(cb("update_enabled_features")?, &["minecraft:vanilla"]),
            cfg::select_known_packs(cb("select_known_packs")?, std::slice::from_ref(&pack)),
        ];

        let registry_id = cb("registry_data")?;
        let mut sync: Vec<Vec<u8>> = registries
            .synchronized
            .iter()
            .map(|r| cfg::registry_data(registry_id, &format!("minecraft:{}", r.name), &r.entries))
            .collect();
        let tag_names: Vec<String> = registries
            .tags
            .iter()
            .map(|t| format!("minecraft:{}", t.registry))
            .collect();
        sync.push(cfg::update_tags(
            cb("update_tags")?,
            tag_names
                .iter()
                .zip(&registries.tags)
                .map(|(name, t)| (name.as_str(), t.tags.as_slice())),
        ));
        sync.push(cfg::finish_configuration(cb("finish_configuration")?));

        Ok(Self {
            ids: Ids {
                disconnect: cb("disconnect")?,
                sb_select_known_packs: sb("select_known_packs")?,
                sb_finish: sb("finish_configuration")?,
                sb_client_information: sb("client_information")?,
                // Нет в урезанных таблицах тестов: -1 не совпадёт ни с одним ID.
                sb_custom_payload: sb("custom_payload").unwrap_or(-1),
            },
            pack,
            greeting,
            sync,
        })
    }

    /// Суммарный размер пакетов синхронизации (для лога).
    pub fn sync_bytes(&self) -> usize {
        self.sync.iter().map(Vec::len).sum()
    }

    pub async fn run(&self, conn: &mut Connection) -> Result<Outcome, NetError> {
        for p in &self.greeting {
            conn.send(p).await?;
        }
        let mut synced = false;
        let mut client = cfg::ClientInfo::DEFAULT;
        let mut channels = Vec::new();
        loop {
            let packet = conn.read_packet().await?;
            let mut r = Reader::new(&packet);
            let id = r.var_int()?;
            if id == self.ids.sb_select_known_packs && !synced {
                let packs = cfg::read_known_packs(&mut r)?;
                if !packs.contains(&self.pack) {
                    // Без общего пака пришлось бы слать полные NBT-данные всех записей.
                    let msg = format!(
                        "Your client does not have the vanilla data pack {}:{}:{}",
                        self.pack.namespace, self.pack.id, self.pack.version
                    );
                    conn.send(&cfg::disconnect(self.ids.disconnect, &msg))
                        .await?;
                    return Ok(Outcome::Rejected);
                }
                for p in &self.sync {
                    conn.send(p).await?;
                }
                synced = true;
            } else if id == self.ids.sb_finish && synced {
                return Ok(Outcome::Finished(client, channels));
            } else if id == self.ids.sb_client_information {
                client = cfg::read_client_information(&mut r)?;
            } else if id == self.ids.sb_custom_payload {
                // Моды объявляют каналы ещё до входа в мир; плагины узнают о них
                // после `PlayerJoinEvent`, как в Paper.
                if r.string(32767)? == "minecraft:register" {
                    channels.extend(crate::payload::register_names(r.remaining()));
                }
            }
            // keep_alive, brand и т.п. пока игнорируются.
        }
    }
}
