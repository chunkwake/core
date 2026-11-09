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
    registry_data: i32,
}

#[derive(Debug)]
pub struct ConfigurationPlan {
    ids: Ids,
    pack: KnownPack,
    /// update_enabled_features + select_known_packs.
    greeting: Vec<Vec<u8>>,
    /// registry_data × N, update_tags, finish_configuration.
    sync: Vec<Vec<u8>>,
    /// Синхронизируемые реестры (имена и записи по порядку сетевых ID).
    registries: Vec<chunkwake_data::registries::Registry>,
    /// Каталог вывода генератора: данные записей для клиента без общего пака.
    data_dir: Option<std::path::PathBuf>,
    /// registry_data с данными записей — собирается при первом таком клиенте.
    full: std::sync::OnceLock<Result<Vec<Vec<u8>>, String>>,
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
                registry_data: registry_id,
            },
            pack,
            greeting,
            sync,
            registries: registries.synchronized.clone(),
            data_dir: None,
            full: std::sync::OnceLock::new(),
        })
    }

    /// Каталог вывода генератора (`data/minecraft/...`): с ним клиент без пака
    /// `minecraft:core` нашей версии (другая версия через ViaVersion, моды)
    /// получает реестры с данными записей, а не отказ.
    pub fn with_data_dir(mut self, dir: &std::path::Path) -> Self {
        self.data_dir = Some(dir.to_path_buf());
        self
    }

    /// registry_data с данными, затем общий хвост (теги, finish).
    fn full_sync(&self) -> Result<&[Vec<u8>], String> {
        let full = self.full.get_or_init(|| {
            let dir = self.data_dir.as_ref().ok_or("no vanilla data directory")?;
            let mut out = Vec::with_capacity(self.sync.len());
            for r in &self.registries {
                let entries = r
                    .entries
                    .iter()
                    .map(|e| {
                        let path = e.strip_prefix("minecraft:").unwrap_or(e);
                        let file = dir.join("data/minecraft").join(&r.name).join(format!("{path}.json"));
                        let text = std::fs::read_to_string(&file)
                            .map_err(|err| format!("{}: {err}", file.display()))?;
                        let json: serde_json::Value = serde_json::from_str(&text)
                            .map_err(|err| format!("{}: {err}", file.display()))?;
                        // Запись — любой тег: у части реестров это строка или список.
                        Ok((e.clone(), chunkwake_world::nbt::write_network_tag(&json_to_nbt(&json))))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                out.push(cfg::registry_data_full(
                    self.ids.registry_data,
                    &format!("minecraft:{}", r.name),
                    &entries,
                ));
            }
            out.extend_from_slice(&self.sync[self.registries.len()..]);
            Ok(out)
        });
        full.as_deref().map_err(Clone::clone)
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
                // Клиент без нашего пака (другая версия через ViaVersion, моды):
                // реестры уходят с данными записей, как у ванилы.
                let full = if packs.contains(&self.pack) {
                    None
                } else {
                    match self.full_sync() {
                        Ok(full) => Some(full),
                        Err(error) => {
                            tracing::warn!("registry data for a client without the vanilla pack: {error}");
                            None
                        }
                    }
                };
                if let Some(full) = full {
                    for p in full {
                        conn.send(p).await?;
                    }
                    synced = true;
                    continue;
                }
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

/// Запись реестра из JSON датапака в NBT — так же, как ванила кодирует её
/// `NbtOps`: числа без дробной части — `Int` (или `Long`), прочие — `Double`
/// (кодеки на клиенте читают любой числовой тег), флаги — `Byte`.
/// Список из разных типов — список compound-ов с ключом `""`, как
/// разнотипные списки NBT с 1.21.5.
fn json_to_nbt(v: &serde_json::Value) -> chunkwake_world::nbt::Tag {
    use chunkwake_world::nbt::Tag;
    use serde_json::Value;
    match v {
        Value::Null => Tag::Compound(Default::default()),
        Value::Bool(b) => Tag::Byte(*b as i8),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i32::try_from(i).map_or(Tag::Long(i), Tag::Int),
            None => Tag::Double(n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => Tag::String(s.clone()),
        Value::Array(a) => {
            let mut items: Vec<Tag> = a.iter().map(json_to_nbt).collect();
            // Числа вперемешку (`[1, 0.5]`) — все `Double`, как один тип.
            if items.iter().all(|t| matches!(t, Tag::Int(_) | Tag::Long(_) | Tag::Double(_))) {
                if items.iter().any(|t| matches!(t, Tag::Double(_))) {
                    for t in &mut items {
                        *t = match *t {
                            Tag::Int(i) => Tag::Double(i as f64),
                            Tag::Long(i) => Tag::Double(i as f64),
                            ref d => d.clone(),
                        };
                    }
                } else if items.iter().any(|t| matches!(t, Tag::Long(_))) {
                    for t in &mut items {
                        if let Tag::Int(i) = *t {
                            *t = Tag::Long(i as i64);
                        }
                    }
                }
            }
            let mixed = items.windows(2).any(|w| w[0].id() != w[1].id());
            if mixed {
                Tag::List(
                    items
                        .into_iter()
                        .map(|t| Tag::Compound([(String::new(), t)].into_iter().collect()))
                        .collect(),
                )
            } else {
                Tag::List(items)
            }
        }
        Value::Object(o) => {
            Tag::Compound(o.iter().map(|(k, v)| (k.clone(), json_to_nbt(v))).collect())
        }
    }
}
