# Kvakk

Rust file sharing app supporting two protocols: Google Quick Share (Android) and LocalSend (iOS/everything else). Unified GUI shows both device types in one grid. AGPL-3.0.

## Build & Run

```bash
brokkr check
brokkr run
RUST_LOG=debug brokkr run  # verbose
```

Strict clippy lints enforced - see `[lints.clippy]` in Cargo.toml. Notable: `unwrap_used = "deny"`, `too_many_lines = "deny"`, `cognitive_complexity = "deny"`.

## Architecture

- `src/main.rs` - egui GUI app (eframe + catppuccin mocha theme, 344x350 fixed window)
- `src/rqs/lib.rs` - Core library, starts tokio runtime, mDNS, BLE, TCP server, LocalSend server
- `src/rqs/manager.rs` - TCP server, connection handling (max 100 concurrent via semaphore)
- `src/rqs/channel.rs` - Message passing between async backend and GUI
- `src/rqs/utils.rs` - Endpoint ID, crypto helpers, mDNS name generation

### Quick Share handlers
- `src/rqs/hdl/inbound.rs` - Inbound transfer state machine, generic over the stream (TCP, or BLE weave via `MigratableStream`)
- `src/rqs/hdl/inbound/bwu.rs` - Wi-Fi LAN bandwidth upgrade for BLE-initiated transfers (Linux-only)
- `src/rqs/hdl/outbound.rs` - Outbound transfer state machine
- `src/rqs/hdl/mdns.rs` - mDNS service registration
- `src/rqs/hdl/mdns_discovery.rs` - mDNS device discovery, `EndpointInfo` and `TransferProtocol` types
- `src/rqs/hdl/ble.rs` - BLE listener (btleplug, cross-platform)
- `src/rqs/hdl/blea.rs` - BLE sender beacon, 0xFE2C (bluer, Linux-only)
- `src/rqs/hdl/blea_windows.rs` - BLE sender beacon (windows crate, Windows-only)
- `src/rqs/hdl/ble_receiver.rs` - BLE receiver advertiser + GATT server, 0xFEF3 (bluer, Linux-only)
- `src/rqs/hdl/weave.rs` - Weave packet codec for the BLE data socket
- `src/rqs/hdl/migratable.rs` - Stream that swaps from BLE weave to TCP on bandwidth upgrade
- `src/rqs/hdl/info.rs` - Transfer metadata and payload structs
- `src/proto_src/*.proto` - Google Quick Share protocol buffer definitions
- `build.rs` - prost-build for proto compilation

### LocalSend handlers
- `src/rqs/localsend/` - Vendored, trimmed copy of `localsend-rs` (client, server, multicast discovery, protocol types)
- `src/rqs/hdl/localsend_discovery.rs` - Wraps the vendored multicast discovery, maps to `EndpointInfo`
- `src/rqs/hdl/localsend_server.rs` - HTTP server bridge for receiving files, auto-accepts, polls `PendingTransfer`
- `src/rqs/hdl/localsend_send.rs` - Outbound file sender via `LocalSendClient`

## Protocols

### Quick Share
1. **Discovery**: mDNS (`_FC9F5ED42C8A._tcp.local.`) + BLE (UUID 0xFE2C sender beacon; UUID 0xFEF3 receiver advertisement on Linux). Since Quick Share's AirDrop-compatibility update, phones can drop off Wi-Fi while the share sheet is open and then only find receivers over 0xFEF3.
2. **Connection**: TCP + UKEY2 handshake (P256 ECDH -> AES-256 + HMAC-SHA256). BLE-discovered phones connect over GATT (weave socket), and the receiver then offers a Wi-Fi LAN bandwidth upgrade so the payload moves over TCP.
3. **Transfer**: Encrypted chunked frames with 4-byte BE length prefix, 5MB frame limit
4. **Completion**: Receiver ACKs payloads, sender requests safe-to-disconnect, receiver initiates disconnect

### LocalSend
1. **Discovery**: UDP multicast (224.0.0.167:53317)
2. **Connection**: HTTP REST API on port 53317
3. **Transfer**: `prepare-upload` -> `upload` per file (streaming, 8KB buffer)
4. Uses an in-tree copy of `localsend-rs` in `src/rqs/localsend/`

Both protocols auto-accept all incoming transfers. Files saved to `~/Downloads`.

## Platform

Cross-platform (Linux, macOS, Windows). BLE advertiser has platform-specific implementations (`bluer` on Linux, `windows` crate on Windows). Everything else is cross-platform. Filters out virtual network interfaces (Docker, Tailscale, WSL2).

Persistent endpoint ID stored at `~/.local/share/kvakk/endpoint_id`.

## Rules

### General rules

- Don't use gremlins! Em-dash, en-dash, strange quotes, whatever - they're all verboten.
- Don't remind the user of the rules. They wrote them, so they know them.
- The user can exempt you from any rule at any time.

### Bash rules

- Never read or write from `/tmp`. All data lives in the project.
- Never run raw `cargo`, `curl`, `pkill`. Use `brokkr`.

## Document folders

The standing layout, across every project. Three live folders plus one retired,
split by durability first, subject second.

| Folder | Contents | Rule |
|---|---|---|
| `reference/` | Durable in-repo reference for anyone working on or with the code - how the thing is built and why: `architecture.md`, `technical-implementation-spec.md`, `performance.md` (the durable record of measured numbers over time), invariants, protocol contracts | Citable from source as a source of truth. What it says must be true. |
| `docs/` | Durable in-repo documentation of how the thing is used - guides, CLI reference, the consumer-facing API surface. Sometimes exposed as a hand-edited VitePress gh-pages site | Same must-be-true rule. |
| `notes/` | Transient - work items (`todo.md`), future plans, hypotheticals, bug reports, research, analysis. Things that will die | No truth guarantee. Nothing durable cites it. |
| `plans/` | Retired | Plan documents are transient: they go in `notes/`. |

`reference/` and `docs/` are both durable and both binding. The difference is
subject, not audience: `reference/` covers how the thing is built and why - what
you need in order to change it safely - while `docs/` covers how it is used. A
developer or library consumer reads both. Where a project publishes a site,
`docs/` is what gets published; the folder means the same thing either way.
`notes/` is neither durable nor binding, which is the whole point of keeping it
separate: a document that may be wrong must not sit where a document that must
be right is expected.

The dependency direction is therefore one-way. `notes/` may cite `docs/` and
`reference/`; nothing durable may cite `notes/` - not a code comment, not
`docs/`, not `reference/`. A code comment must carry its full context, because
it outlives the note.

**Root-level convention files are exempt.** `AGENTS.md`, `CLAUDE.md`,
`README.md`, `LICENSE`, `CHANGELOG.md` and their kin are found by tooling and by
convention at the repository root, and stay there. These folders govern
documents we chose where to put, not files whose location is dictated.

In `notes/`, `docs/` and `reference/` alike, avoid citing source line numbers -
they drift fast.
