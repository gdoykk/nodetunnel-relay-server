use nodetunnel_protocol::ClientId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferChannel {
    Reliable,
    Unreliable,
}

#[derive(Debug, Clone)]
pub enum ServerEvent {
    ClientConnected { client_id: ClientId },
    ClientDisconnected { client_id: ClientId },
    PacketReceived { client_id: ClientId, data: Vec<u8>, channel: TransferChannel },
}
