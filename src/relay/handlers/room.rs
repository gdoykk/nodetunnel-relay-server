use tracing::warn;
use nodetunnel_protocol::packet::{Packet, RoomInfo};
use nodetunnel_protocol::ClientId;
use crate::relay::apps::Apps;
use crate::relay::clients::{ClientState, Clients};
use crate::relay::handlers::sender::PacketSender;
use crate::relay::ids::{AppId, RoomId};
use crate::udp::common::TransferChannel;
use crate::udp::paper_interface::PaperInterface;

pub struct RoomHandler<'a> {
    udp: &'a mut PaperInterface,
    apps: &'a mut Apps,
    clients: &'a mut Clients,
}

impl PacketSender for RoomHandler<'_> {
    fn udp_mut(&mut self) -> &mut PaperInterface {
        self.udp
    }
}

impl<'a> RoomHandler<'a> {
    pub fn new(
        udp: &'a mut PaperInterface,
        apps: &'a mut Apps,
        clients: &'a mut Clients,
    ) -> Self {
        Self {
            udp,
            apps,
            clients
        }
    }

    pub async fn create_room(&mut self, sender_id: ClientId, app_id: AppId, is_public: bool, metadata: &str) {
        let Some(app) = self.apps.get_mut(app_id) else {
            warn!("attempted to create a room for a missing app: {app_id}");
            return;
        };

        let Some(client) = self.clients.get_mut(sender_id) else {
            warn!("attempted to create a room for a missing client: {sender_id}");
            return;
        };

        let room = app.rooms.create(sender_id, is_public, metadata.to_string());
        let join_code = room.join_code.clone();
        let peer_id = room.add_peer(sender_id);
        let room_id = room.id;

        client.state = ClientState::InRoom { app_id, room_id };

        self.send_packet(
            sender_id,
            &Packet::ConnectedToRoom {
                room_id: join_code,
                peer_id,
            },
            TransferChannel::Reliable,
        ).await;
    }

    pub async fn send_rooms(&mut self, target: ClientId, app_id: AppId) {
        let Some(app) = self.apps.get_mut(app_id) else {
            warn!("attempted to list rooms for a missing app: {app_id}");
            return;
        };

        let public_rooms: Vec<RoomInfo> = app.rooms.iter_mut()
            .filter(|room| room.is_public)
            .map(|room| room.to_info())
            .collect();

        self.send_packet(
            target,
            &Packet::GetRooms {
                rooms: public_rooms
            },
            TransferChannel::Reliable,
        ).await;
    }

    pub async fn update_room(&mut self, sender_id: ClientId, app_id: AppId, room_id: RoomId, metadata: &str) {
        let Some(app) = self.apps.get_mut(app_id) else {
            // The client's own state pointed at this app_id, so if it's
            // missing that's a server-side invariant violation (e.g. an app
            // was removed while clients still referenced it). Warn and
            // report back to the client rather than panicking the process.
            warn!("client {sender_id} had app_id {app_id} in its own state, but the app no longer exists");
            self.send_err(sender_id, "Internal error: app no longer exists").await;
            return;
        };

        let Some(room) = app.rooms.get_mut(room_id) else {
            self.send_err(sender_id, "Room not found").await;
            return;
        };

        room.metadata = metadata.to_string();
    }

    pub fn remove_room(&mut self, app_id: AppId, room_id: RoomId) {
        if let Some(app) = self.apps.get_mut(app_id) {
            app.rooms.remove(room_id);
        }
    }

    pub(crate) async fn recv_join_req(&mut self, sender_id: ClientId, app_id: AppId, room_id: &str, metadata: &str) {
        let host_id = {
            let Some(app) = self.apps.get_mut(app_id) else {
                warn!("attempted to handle join request for a missing app: {app_id}");
                return;
            };

            let Some(room) = app.rooms.get_by_jc(room_id) else {
                self.send_err(sender_id, "Room not found").await;
                return;
            };

            room.get_host()
        };

        self.send_packet(
            host_id,
            &Packet::PeerJoinAttempt {
                target_id: sender_id,
                metadata: metadata.to_string()
            },
            TransferChannel::Reliable
        ).await;
    }

    pub(crate) async fn recv_join_res(&mut self, app_id: AppId, target_id: ClientId, room_id: RoomId, allowed: bool) {
        if !allowed {
            self.send_err(target_id, "Room host denied entry").await;
            return;
        }

        let Some(client) = self.clients.get_mut(target_id) else {
            warn!("attempted to handle join response for a missing client: {target_id}");
            return;
        };

        let Some(app) = self.apps.get_mut(app_id) else {
            warn!("host's app_id {app_id} no longer exists when accepting {target_id}");
            self.send_err(target_id, "Internal error: app no longer exists").await;
            return;
        };

        let (peer_id, host_id, join_code) = {
            let Some(room) = app.rooms.get_mut(room_id) else {
                self.send_err(target_id, "Room not found").await;
                return;
            };

            let peer_id = room.add_peer(target_id);
            let host_id = room.get_host();

            (peer_id, host_id, room.join_code.clone())
        };

        client.state = ClientState::InRoom { app_id, room_id };

        self.send_packet(
            target_id,
            &Packet::ConnectedToRoom {
                room_id: join_code,
                peer_id,
            },
            TransferChannel::Reliable,
        ).await;

        self.send_packet(
            host_id,
            &Packet::PeerJoinedRoom {
                peer_id,
            },
            TransferChannel::Reliable
        ).await;
    }
}
