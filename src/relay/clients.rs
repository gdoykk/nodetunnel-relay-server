use std::collections::HashMap;
use nodetunnel_protocol::ClientId;
use crate::relay::ids::{AppId, RoomId};

/// An enum to store different states that a client can be in.
/// Defaults to `Connected`
#[derive(Clone, Default)]
pub enum ClientState {
    #[default]
    Connected,
    Authenticated { app_id: AppId },
    InRoom { app_id: AppId, room_id: RoomId }
}

/// Stores data about a client.
/// See: `ClientState`
#[derive(Default)]
pub struct Client {
    pub state: ClientState,
}

impl Client {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Stores all clients that are connected to the relay server.
/// Provides methods to create, remove, and fetch clients.
#[derive(Default)]
pub struct Clients {
    by_id: HashMap<ClientId, Client>,
}

impl Clients {
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new client with the given ID.
    pub fn create(&mut self, id: ClientId) {
        self.by_id.insert(id, Client::new());
    }

    /// Removes a client with the given ID.
    /// Returns the removed client (if it existed).
    pub fn remove(&mut self, id: ClientId) -> Option<Client> {
        self.by_id.remove(&id)
    }

    /// Gets a reference to a client by ID.
    pub fn get(&self, id: ClientId) -> Option<&Client> {
        self.by_id.get(&id)
    }

    /// Gets a mutable reference to a client by ID.
    pub fn get_mut(&mut self, id: ClientId) -> Option<&mut Client> {
        self.by_id.get_mut(&id)
    }
}
