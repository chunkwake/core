//! `client_information` в play (`ServerGamePacketListenerImpl.handleClientInformation`).
//!
//! Клиент шлёт его при смене настроек. Ванила:
//! - шляпа сменилась → всем `player_info_update` с `UPDATE_HAT`;
//! - `ServerPlayer.updateOptions` ставит руку и части скина в `SynchedEntityData`:
//!   изменённые значения уходят следящим и самому игроку;
//! - дальность обзора (`requestedViewDistance`) меняет `getPlayerViewDistance`,
//!   поток чанков пересобирается под новый радиус.

use super::*;
use chunkwake_protocol::configuration::ClientInfo;

impl PlayPlan {
    /// Сохраняет настройки в записи игрока и рассылает изменения модели.
    pub(super) fn client_information(&self, uuid: Uuid, entity_id: i32, info: &ClientInfo) {
        let old = {
            let mut players = self.hub.players.lock().unwrap();
            let Some(p) = players.get_mut(&uuid) else {
                return;
            };
            let old = (p.model, p.entry.show_hat);
            p.model = [info.main_hand, info.skin_parts];
            p.view_distance = info.view_distance;
            p.entry.show_hat = info.shows_hat();
            old
        };
        let ([hand, skin], hat) = old;
        if hat != info.shows_hat() {
            self.hub.send_global(play::player_info_hat(
                self.ids.player_info_update,
                uuid,
                info.shows_hat(),
            ));
        }
        let hand = (hand != info.main_hand).then_some(info.main_hand);
        let skin = (skin != info.skin_parts).then_some(info.skin_parts);
        if hand.is_some() || skin.is_some() {
            // `broadcastAndSend`: следящим и себе.
            self.hub.send(
                None,
                inv::set_player_model(self.ids.set_entity_data, entity_id, hand, skin),
            );
        }
    }

    /// Новый радиус обзора: выгрузить кольцо за ним или догрузить новое.
    pub(super) async fn stream_resize(
        &self,
        conn: &mut Connection,
        s: &mut stream::ChunkStream,
        radius: i32,
    ) -> Result<(), NetError> {
        let (c, old) = (s.center(), s.radius());
        if radius == old {
            return Ok(());
        }
        let forget = chunks_around(c, old, Some((c, radius)));
        let fresh = chunks_around(c, radius, Some((c, old)));
        s.set_radius(radius);
        self.stream_enter(conn, s, c, &forget, fresh).await
    }
}
