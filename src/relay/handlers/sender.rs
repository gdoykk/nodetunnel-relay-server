use nodetunnel_protocol::packet::Packet;
use nodetunnel_protocol::ClientId;
use tracing::warn;
use crate::udp::common::TransferChannel;
use crate::udp::paper_interface::PaperInterface;

/// Standard error code used for authorization/validation failures reported
/// to clients via `Packet::Error`.
pub const ERR_UNAUTHORIZED: i32 = 401;

/// Shared helper for sending packets to clients from a handler.
///
/// Every handler needs to serialize a `Packet` and hand it to the UDP
/// transport, then log (rather than propagate) transport failures, since a
/// single client's dead/unreachable socket must never abort handling for
/// other clients. This trait consolidates that behavior so each handler
/// doesn't redefine its own copy of `send_packet`/`send_err`.
pub trait PacketSender {
    fn udp_mut(&mut self) -> &mut PaperInterface;

    /// Sends `packet` to `target`. Failures are logged and swallowed: a
    /// send failure to one client must not interrupt handling of the
    /// current event for anyone else.
    fn send_packet(
        &mut self,
        target: ClientId,
        packet: &Packet,
        channel: TransferChannel,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        async move {
            if let Err(e) = self.udp_mut().send(target, packet.to_bytes(), channel).await {
                warn!("failed to send {:?} to {target}: {e}", packet.kind());
            }
        }
    }

    /// Sends a `Packet::Error` with `ERR_UNAUTHORIZED` to `target`.
    fn send_err(
        &mut self,
        target: ClientId,
        msg: &str,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        let packet = Packet::Error {
            error_code: ERR_UNAUTHORIZED,
            error_message: msg.to_string(),
        };

        async move {
            self.send_packet(target, &packet, TransferChannel::Reliable).await;
        }
    }

    /// Sends `Packet::ForceDisconnect` to `target` and immediately tears
    /// down its UDP session.
    fn force_disconnect(
        &mut self,
        target: ClientId,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        async move {
            self.send_packet(target, &Packet::ForceDisconnect, TransferChannel::Reliable).await;
            self.udp_mut().remove_client(target);
        }
    }
}
