use tracing::{info, warn};
use nodetunnel_protocol::packet::Packet;
use nodetunnel_protocol::ClientId;
use crate::relay::apps::Apps;
use crate::relay::clients::{ClientState, Clients};
use crate::relay::handlers::room::RoomHandler;
use crate::relay::handlers::sender::PacketSender;
use crate::relay::ids::{AppId, RoomId};
use crate::udp::common::TransferChannel;
use crate::udp::paper_interface::PaperInterface;

struct DisconnectInfo {
    is_host: bool,
    godot_id: i32,
    other_peers: Vec<ClientId>,
}

pub struct DisconnectHandler<'a> {
    udp: &'a mut PaperInterface,
    clients: &'a mut Clients,
    apps: &'a mut Apps,
}

impl PacketSender for DisconnectHandler<'_> {
    fn udp_mut(&mut self) -> &mut PaperInterface {
        self.udp
    }
}

impl<'a> DisconnectHandler<'a> {
    pub fn new(
        udp: &'a mut PaperInterface,
        clients: &'a mut Clients,
        apps: &'a mut Apps,
    ) -> Self {
        Self {
            udp,
            clients,
            apps,
        }
    }

    pub async fn handle_disconnect(&mut self, client_id: ClientId) {
        let Some(client) = self.clients.remove(client_id) else {
            warn!("unregistered client disconnected");
            return;
        };

        if let ClientState::InRoom { app_id, room_id } = client.state {
            self.handle_room_disconnect(client_id, app_id, room_id).await;
        }
    }

    async fn handle_room_disconnect(&mut self, sender_id: ClientId, app_id: AppId, room_id: RoomId) {
        let disconnect_info = {
            let Some(app) = self.apps.get_mut(app_id) else {
                warn!("{sender_id} had invalid app_id on disconnect");
                return;
            };

            let Some(room) = app.rooms.get(room_id) else {
                warn!("{sender_id} had invalid room_id on disconnect");
                return;
            };

            let Some(godot_id) = room.client_to_gd(sender_id) else {
                warn!("{sender_id} not found in their room on disconnect");
                return;
            };

            DisconnectInfo {
                is_host: room.get_host() == sender_id,
                godot_id,
                other_peers: room.get_clients()
                    .into_iter()
                    .filter(|&id| id != sender_id)
                    .collect(),
            }
        };

        if disconnect_info.is_host {
            self.handle_host_disconnect(app_id, room_id, disconnect_info.other_peers).await;
        } else {
            self.handle_peer_disconnect(app_id, room_id, sender_id, disconnect_info.godot_id, disconnect_info.other_peers).await;
        }
    }

    async fn handle_host_disconnect(&mut self, app_id: AppId, room_id: RoomId, peers_to_kick: Vec<ClientId>) {
        info!("host disconnected");
        RoomHandler::new(
            self.udp,
            self.apps,
            self.clients,
        ).remove_room(app_id, room_id);

        for peer_id in peers_to_kick {
            self.clients.remove(peer_id);
            self.force_disconnect(peer_id).await;
        }
    }

    async fn handle_peer_disconnect(&mut self, app_id: AppId, room_id: RoomId, client_id: ClientId, peer_godot_id: i32, other_peers: Vec<ClientId>) {
        info!("peer disconnected");
        if let Some(app) = self.apps.get_mut(app_id)
            && let Some(room) = app.rooms.get_mut(room_id) {
            room.remove_peer(client_id);
        }

        for peer_id in other_peers {
            self.send_packet(peer_id, &Packet::PeerLeftRoom { peer_id: peer_godot_id }, TransferChannel::Reliable).await;
        }
    }
}
