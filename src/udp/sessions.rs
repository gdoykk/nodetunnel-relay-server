use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use nodetunnel_protocol::ClientId;
use paperudp::channel::Channel;

pub struct ClientSession {
    pub id: ClientId,
    pub addr: SocketAddr,
    pub channel: Channel,
    pub last_heard_from: Instant,
}

pub struct ConnectionManager {
    id_to_session: HashMap<ClientId, ClientSession>,
    addr_to_id: HashMap<SocketAddr, ClientId>,
    next_client_id: u64,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            id_to_session: HashMap::new(),
            addr_to_id: HashMap::new(),
            next_client_id: 1,
        }
    }

    /// Returns the `ClientSession` for `addr`, creating one if it doesn't
    /// already exist, and whether the session was newly created.
    pub fn get_or_create(&mut self, addr: SocketAddr) -> (&mut ClientSession, bool) {
        let existing_id = self.addr_to_id.get(&addr).copied();

        match existing_id {
            // We just looked `id` up from `addr_to_id`, which is always
            // kept in sync with `id_to_session`, so this entry is
            // guaranteed to exist.
            Some(id) if self.id_to_session.contains_key(&id) => {
                (self.id_to_session.get_mut(&id).unwrap_or_else(|| {
                    unreachable!("just checked id_to_session contains {id}")
                }), false)
            }
            _ => (self.create_session(addr), true),
        }
    }

    pub fn create_session(&mut self, addr: SocketAddr) -> &mut ClientSession {
        let id = ClientId::new(self.next_client_id);
        self.next_client_id += 1;

        let session = ClientSession {
            id,
            addr,
            channel: Channel::new(),
            last_heard_from: Instant::now(),
        };

        self.id_to_session.insert(id, session);
        self.addr_to_id.insert(addr, id);

        // Safe to index directly: we just inserted this key above.
        self.id_to_session
            .get_mut(&id)
            .unwrap_or_else(|| unreachable!("session was just inserted under id {id}"))
    }

    pub fn get_by_id(&mut self, id: ClientId) -> Option<&mut ClientSession> {
        self.id_to_session.get_mut(&id)
    }

    pub fn get_resends(&mut self, interval: Duration) -> Vec<(SocketAddr, Vec<u8>)> {
        let mut out = Vec::new();

        for session in self.id_to_session.values_mut() {
            let packets = session.channel.collect_resends(interval);

            for pkt in packets {
                out.push((session.addr, pkt));
            }
        }

        out
    }

    pub fn cleanup_sessions(&mut self, timeout: Duration) -> Vec<ClientId> {
        let now = Instant::now();
        let mut expired = Vec::new();

        for (&id, session) in &self.id_to_session {
            if now.duration_since(session.last_heard_from) > timeout {
                expired.push(id);
            }
        }

        for &id in &expired {
            if let Some(session) = self.id_to_session.remove(&id) {
                self.addr_to_id.remove(&session.addr);
            }
        }

        expired
    }

    pub fn remove_session(&mut self, id: ClientId) {
        if let Some(session) = self.id_to_session.remove(&id) {
            self.addr_to_id.remove(&session.addr);
        }
    }
}
