# relay-server

Tokio-based UDP relay server for [NodeTunnel](../godot-plugin). Routes
authentication, room-management, and `GameData` packets between connected
Godot clients so games can do peer-to-peer multiplayer without dedicated
servers or port-forwarding.

## How it works

Each connected client is a UDP session tracked by `nodetunnel-protocol`'s
`ClientId` and moves through three states, enforced in
`src/relay/server.rs::handle_packet`:

1. **Connected** — freshly seen on the transport, not yet authenticated.
   Only `Authenticate` is accepted.
2. **Authenticated** (`app_id`) — passed the whitelist/version check.
   Can `CreateRoom`, request to join a room (`ReqJoin`), or list public
   rooms (`ReqRooms`).
3. **InRoom** (`app_id`, `room_id`) — can send `GameData`, update room
   metadata (`UpdateRoom`), and respond to join requests (`JoinRes`).

Any packet that doesn't match the client's current state is rejected with
an `ErrorPacket` reply instead of being silently dropped (see
`reject_unexpected_packet`).

Reliability and ordering on top of raw UDP (channels, acks, resends) come
from [`paperudp`](../paperudp) via `PaperInterface`
(`src/udp/paper_interface.rs`); wire packet encoding/decoding comes from
[`nodetunnel-protocol`](../protocol). The server loop
(`RelayServer::run`) multiplexes incoming UDP events with two periodic
tasks:

- session cleanup, dropping clients that haven't sent traffic in 5s
- reliable-packet resends, retried every 50ms after a 100ms ack timeout

### Module layout

- `src/main.rs` — process entry point: loads config, binds the UDP socket,
  runs the server until `SIGINT` or a fatal transport error.
- `src/relay/server.rs` — `RelayServer`, the top-level event loop and
  per-state packet routing.
- `src/relay/handlers/` — one handler per concern: `auth`, `room`,
  `game_data`, `disconnect`, plus `sender` (shared `PacketSender` trait
  for replying to clients).
- `src/relay/{apps,clients,rooms,ids}.rs` — in-memory state: apps (by app
  ID), connected clients, rooms within an app, and the newtypes
  (`AppId`, `RoomId`) used to key them.
- `src/udp/` — the transport layer: `PaperInterface` (wraps `paperudp`
  channels per client), `sessions.rs` (UDP session tracking/timeouts),
  `common.rs` (shared event/channel types).
- `src/config/` — `Config` struct, defaults, and the TOML/env loader.

## Configuration

Config is loaded by `src/config/loader.rs`: if `config.toml` exists next
to the binary it's used, otherwise config comes from environment
variables (optionally via a `.env` file, loaded with `dotenvy`). A
malformed or partial config is a **hard error** at startup rather than a
silent fallback to defaults — this matters because a mistyped `WHITELIST`
or `ALLOWED_VERSIONS` should not silently turn into "allow everything".

See `.env.example` for the full list of variables:

| Variable | Purpose |
| --- | --- |
| `UDP_BIND_ADDRESS` | Address the relay listens on (default `0.0.0.0:8080`). |
| `ALLOWED_VERSIONS` | Comma-separated client `PROTOCOL_VERSION` strings allowed to connect. |
| `WHITELIST` | Comma-separated local app IDs allowed to connect. |
| `REMOTE_WHITELIST_ENDPOINT` | Optional HTTP endpoint to check app-ID allowlisting remotely instead of `WHITELIST`. |
| `REMOTE_WHITELIST_TOKEN` | Bearer token sent to the remote whitelist endpoint. |
| `WHITELIST_FAILURE_POLICY` | `fail_closed` (default, reject on remote-whitelist errors) or `fail_open_to_local` (fall back to `WHITELIST`). |
| `RELAY_ID` | Prefix applied to generated room IDs, to identify which relay a room lives on. |

## Building and running

```
cargo build
cargo test
cargo clippy
cargo fmt
```

Locally:

```
cp .env.example .env   # edit values as needed
cargo run --release
```

### Docker

```
docker compose up --build
```

`docker-compose.yml` builds from the included `Dockerfile` (multi-stage,
`rust:1.97-trixie` → `debian:trixie-slim`), reads config from `.env`, and
exposes `8080/udp` (game traffic) and `8081/tcp` (reserved). Restart
policy is `unless-stopped`.

## Notes

- This crate depends on `nodetunnel-protocol` and `paperudp` via git
  dependencies (see `Cargo.toml`), not local paths — unlike
  `godot-plugin`, which uses a local path for `paperudp`. Watch for
  version drift between the two when changing either shared crate; see
  the repo-level `AGENTS.md` for details.
- Protocol/packet-layout changes must stay in sync with
  [`godot-plugin`](../godot-plugin), since both talk the same wire format
  defined in `nodetunnel-protocol`.
