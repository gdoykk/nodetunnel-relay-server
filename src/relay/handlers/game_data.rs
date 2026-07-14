use tracing::warn;
use nodetunnel_protocol::packet::Packet;
use nodetunnel_protocol::ClientId;
use crate::relay::apps::Apps;
use crate::relay::handlers::sender::PacketSender;
use crate::relay::ids::{AppId, RoomId};
use crate::udp::common::TransferChannel;
use crate::udp::paper_interface::PaperInterface;

pub struct GameDataHandler<'a> {
    udp: &'a mut PaperInterface,
    apps: &'a mut Apps,
}

impl PacketSender for GameDataHandler<'_> {
    fn udp_mut(&mut self) -> &mut PaperInterface {
        self.udp
    }
}

impl<'a> GameDataHandler<'a> {
    pub fn new(
        udp: &'a mut PaperInterface,
        apps: &'a mut Apps
    ) -> Self {
        Self {
            udp,
            apps,
        }
    }

    pub async fn route_game_data(&mut self, sender_id: ClientId, client_app_id: AppId, client_room_id: RoomId, target_peer: i32, data: &[u8], channel: &TransferChannel) {
        let Some(app) = self.apps.get_mut(client_app_id) else {
            warn!("{sender_id} has invalid app_id in index");
            return;
        };

        let Some(room) = app.rooms.get(client_room_id) else {
            warn!("{sender_id} has invalid room_id in index");
            return;
        };

        let Some(sender_godot_id) = room.client_to_gd(sender_id) else {
            warn!("{sender_id} not found in their own room");
            return;
        };

        let Some(target_client_id) = room.gd_to_client(target_peer) else {
            return;
        };

        self.send_packet(
            target_client_id,
            &Packet::GameData {
                from_peer: sender_godot_id,
                data: data.to_vec(),
            },
            *channel,
        ).await;
    }
}
