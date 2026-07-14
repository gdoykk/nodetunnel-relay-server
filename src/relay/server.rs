use std::error::Error;
use std::time::Duration;
use nodetunnel_protocol::packet::Packet;
use nodetunnel_protocol::ClientId;
use tracing::{debug, info, warn};
use crate::config::loader::Config;
use crate::relay::apps::Apps;
use crate::relay::clients::{ClientState, Clients};
use crate::relay::handlers::auth::AuthHandler;
use crate::relay::handlers::disconnect::DisconnectHandler;
use crate::relay::handlers::game_data::GameDataHandler;
use crate::relay::handlers::room::RoomHandler;
use crate::relay::handlers::sender::PacketSender;
use crate::relay::ids::{AppId, RoomId};
use crate::udp::common::{TransferChannel, ServerEvent};
use crate::udp::paper_interface::PaperInterface;

/// How often expired UDP sessions are swept.
const SESSION_CLEANUP_INTERVAL: Duration = Duration::from_secs(1);
/// A session is considered expired after this long without any traffic.
const SESSION_TIMEOUT: Duration = Duration::from_secs(5);
/// How often unacknowledged reliable packets are checked for resend.
const RESEND_CHECK_INTERVAL: Duration = Duration::from_millis(50);
/// How long to wait for an ack before resending a reliable packet.
const RESEND_AFTER: Duration = Duration::from_millis(100);

pub struct RelayServer {
    udp: PaperInterface,
    http_client: reqwest::Client,

    config: Config,
    apps: Apps,
    clients: Clients,
}

impl RelayServer {
    pub fn new(transport: PaperInterface, config: Config) -> Self {
        Self {
            udp: transport,
            http_client: reqwest::Client::new(),
            config,
            apps: Apps::new(),
            clients: Clients::new(),
        }
    }

    /// Starts the server loop.
    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        let mut cleanup = tokio::time::interval(SESSION_CLEANUP_INTERVAL);
        let mut resend = tokio::time::interval(RESEND_CHECK_INTERVAL);

        cleanup.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        resend.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                result = self.udp.recv_events() => {
                    let events = result?;
                    for event in events {
                        self.handle_event(event).await;
                    }
                }

                _ = cleanup.tick() => {
                    for client_id in self.udp.connection_manager.cleanup_sessions(SESSION_TIMEOUT) {
                        self.handle_event(ServerEvent::ClientDisconnected { client_id }).await;
                    }
                }

                _ = resend.tick() => {
                    self.udp.do_resends(RESEND_AFTER).await;
                }
            }
        }
    }

    /// Handles an event from the UDP layer.
    async fn handle_event(&mut self, event: ServerEvent) {
        match event {
            ServerEvent::ClientConnected { client_id } => {
                self.clients.create(client_id);
            }
            ServerEvent::ClientDisconnected { client_id } => {
                DisconnectHandler::new(
                    &mut self.udp,
                    &mut self.clients,
                    &mut self.apps,
                ).handle_disconnect(client_id).await;
            }
            ServerEvent::PacketReceived { client_id, data, channel } => {
                debug!("got packet: {:?}", data);
                self.handle_packet(client_id, data, channel).await;
            }
        }
    }

    /// Handles a packet received from `PaperUDP`.
    /// This checks the state of the client and routes packets based on the state.
    async fn handle_packet(&mut self, from_client_id: ClientId, data: Vec<u8>, channel: TransferChannel) {
        let Some(client) = self.clients.get(from_client_id) else {
            // This means that the client is not in the list of connected clients.
            // Likely a bug in the client or a malicious client.
            warn!("received a packet from an invalid peer");
            return;
        };

        let Ok(packet) = Packet::from_bytes(&data) else {
            warn!("received an invalid packet from {from_client_id}");
            return;
        };

        match client.state {
            ClientState::Connected => self.handle_unauthenticated_packet(from_client_id, &packet).await,
            ClientState::Authenticated { app_id } => self.handle_authenticated_packet(from_client_id, app_id, &packet).await,
            ClientState::InRoom { app_id, room_id } => self.handle_in_room_packet(from_client_id, app_id, room_id, &packet, &channel).await
        }
    }

    /// Delegates packets to various handlers when the client has yet to authenticate.
    async fn handle_unauthenticated_packet(&mut self, from_client_id: ClientId, packet: &Packet) {
        match packet {
            Packet::Authenticate { app_id, version } => {
                AuthHandler::new(
                    &mut self.udp,
                    &self.http_client,
                    &mut self.clients,
                    &mut self.apps,
                    &self.config
                ).authenticate_client(from_client_id, app_id, version).await;
            }
            _ => {
                self.reject_unexpected_packet(from_client_id, packet, "authenticate first").await;
            }
        }
    }

    /// Delegates packets to various handlers when the client is authenticated, but not in a room.
    async fn handle_authenticated_packet(&mut self, from_client_id: ClientId, client_app_id: AppId, packet: &Packet) {
        match packet {
            Packet::CreateRoom { is_public, metadata } =>
                RoomHandler::new(&mut self.udp, &mut self.apps, &mut self.clients)
                    .create_room(from_client_id, client_app_id, *is_public, metadata).await,
            Packet::ReqJoin { room_id, metadata } =>
                RoomHandler::new(&mut self.udp, &mut self.apps, &mut self.clients)
                    .recv_join_req(from_client_id, client_app_id, room_id, metadata).await,
            Packet::ReqRooms =>
                RoomHandler::new(&mut self.udp, &mut self.apps, &mut self.clients)
                    .send_rooms(from_client_id, client_app_id).await,
            _ => {
                self.reject_unexpected_packet(from_client_id, packet, "join or create a room first").await;
            }
        }
    }

    /// Delegates packets to various handlers when the client is in a room.
    async fn handle_in_room_packet(&mut self, from_client_id: ClientId, client_app_id: AppId, client_room_id: RoomId, packet: &Packet, channel: &TransferChannel) {
        match packet {
            Packet::UpdateRoom { metadata, room_id: _room_id } => {
                RoomHandler::new(
                    &mut self.udp,
                    &mut self.apps,
                    &mut self.clients,
                ).update_room(from_client_id, client_app_id, client_room_id, metadata).await;
            }
            Packet::JoinRes { target_id, allowed, room_id: _room_id } =>
                RoomHandler::new(
                    &mut self.udp,
                    &mut self.apps,
                    &mut self.clients,
                ).recv_join_res(client_app_id, *target_id, client_room_id, *allowed).await,
            Packet::GameData { from_peer, data } => {
                GameDataHandler::new(
                    &mut self.udp,
                    &mut self.apps,
                ).route_game_data(from_client_id, client_app_id, client_room_id, *from_peer, data, channel).await;
            }
            _ => {
                self.reject_unexpected_packet(from_client_id, packet, "an unexpected state").await;
            }
        }
    }

    /// Logs and notifies a client that sent a packet type that isn't valid
    /// for their current connection state, instead of silently dropping it.
    async fn reject_unexpected_packet(&mut self, from_client_id: ClientId, packet: &Packet, hint: &str) {
        warn!(
            "unexpected packet {:?} from {from_client_id} ({hint})",
            packet.kind()
        );

        AdHocSender(&mut self.udp)
            .send_err(from_client_id, &format!("Unexpected packet for current state; {hint}."))
            .await;
    }

    /// Forcefully disconnects all clients from the server.
    /// Should be called when the server shuts down.
    pub async fn cleanup(&mut self) {
        let mut disconnects: Vec<ClientId> = Vec::new();
        let mut to_remove: Vec<(AppId, RoomId)> = Vec::new();

        for app in self.apps.iter() {
            for room in app.rooms.iter() {
                disconnects.extend(room.get_clients().iter().copied());
                to_remove.push((app.id, room.id));
            }
        }

        info!("disconnecting {} peers", disconnects.len());

        let mut dh = DisconnectHandler::new(
            &mut self.udp,
            &mut self.clients,
            &mut self.apps
        );

        for id in disconnects {
            dh.force_disconnect(id).await;
        }

        let mut rh = RoomHandler::new(
            &mut self.udp,
            &mut self.apps,
            &mut self.clients,
        );

        for (app_id, room_id) in to_remove {
            rh.remove_room(app_id, room_id);
        }
    }
}

/// Minimal `PacketSender` over a bare `&mut PaperInterface`, used for
/// one-off error replies (e.g. "unexpected packet for your current state")
/// that don't belong to any particular handler.
struct AdHocSender<'a>(&'a mut PaperInterface);

impl PacketSender for AdHocSender<'_> {
    fn udp_mut(&mut self) -> &mut PaperInterface {
        self.0
    }
}
