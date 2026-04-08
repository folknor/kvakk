# LocalSend Issues

Environment: kvakk on ethernet desktop (192.168.0.178), phones on wifi, same router.

## Issue 1 — iPhone doesn't discover kvakk

kvakk sees the iPhone via multicast discovery, but the iPhone never shows kvakk.

### Investigation

- Multicast goes out correct interface (enp9s0, 192.168.0.178)
- Firewall is open (iptables INPUT policy ACCEPT)
- When kvakk receives an announcement from the iPhone, `localsend-rs` calls `respond_to_announcement` which first tries HTTP registration (POST to phone's `/api/localsend/v2/register`) then falls back to a UDP multicast response
- kvakk announces `Protocol::Http` — the official LocalSend app may expect or prefer HTTPS
- The `DeviceInfo` sent in both the HTTP registration body and the UDP announcement has `ip: None` (localsend-rs never populates this field)
- Possibly the same root cause as Issue 3 (phone can't reach kvakk:53317 to complete the handshake)

### Next steps

- Run `RUST_LOG=debug cargo run` and check discovery logs when iPhone is on network
- Verify whether iOS LocalSend requires HTTPS peers
- Check if `ip: None` in registration body causes the phone to discard kvakk

## Issue 2 — Progress bar stuck at 0% when sending to Android (FIXED)

When sending a file to an Android phone, the transfer completes successfully on the phone side, but kvakk's progress bar stays at 0% and the "Sending to device" label never changes.

### Root cause

`emit_initial()` in `localsend_send.rs` set `total_bytes: 0` and subsequent state messages (`SendingFiles`, `Finished`) sent `metadata: None`, so the GUI never received updated byte counts. The `Finished` state was emitted but without metadata to update the progress bar to 100%.

### Fix

- Compute `total_bytes` from file metadata before upload starts
- Track `ack_bytes` per completed file during the upload loop
- Send metadata with both per-file progress updates and the final `Finished` state
- Added `emit_progress()` and `emit_finished()` helpers that include metadata

Note: `localsend-rs` has no mid-stream progress callback (marked TODO in library), so progress updates are per-completed-file only.

## Issue 3 — Android phone can't send to kvakk (connection refused)

When the Android phone tries to send a file to kvakk, it shows:
```
hyper_util::client::legacy::Error(Connect, ConnectError("tcp connect error",
  Os { code: 111, kind: ConnectionRefused }))
```

### Investigation

- `LocalSendServer` binds `0.0.0.0:53317` TCP via axum — should accept connections from any interface
- Firewall is open
- Server startup success is tracked by `localsend_ok` flag and shown as a status LED in the title bar
- UDP (multicast discovery) and TCP (HTTP server) both use port 53317 — this is fine on Linux (separate protocol namespaces)
- Error is `ConnectionRefused` (ECONNREFUSED), not a TLS handshake error — this means nothing was listening on the target IP:port, not a protocol mismatch
- The `hyper_util` in the error string suggests the phone runs a Rust-based LocalSend client

### Next steps

- Run `RUST_LOG=debug cargo run` and attempt send from phone — check for:
  - "LocalSendServer started on port 53317" confirming the server bound successfully
  - Any incoming connection attempts in the logs
- Verify with `ss -tlnp | grep 53317` while kvakk is running that the port is actually held
- Check what IP address the phone is trying to connect to (could be extracting wrong IP from multicast source)
