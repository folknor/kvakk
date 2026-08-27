# AirDrop Interoperability Research

Research conducted 2026-03-23 on adding AirDrop receive support to Kvakk,
enabling file transfers from iPhones.

## Conclusion: Not Viable

The primary target for Kvakk is Windows users receiving files from iPhones.
AirDrop cannot work on Windows - Apple doesn't provide a client, and the
underlying AWDL transport requires low-level Wi-Fi APIs that only exist on
Linux (and even there, require specialized hardware with monitor mode and
frame injection). Google's own Quick Share AirDrop interop is Android-only
for the same reason.

This is a dead end for our use case. The research below is preserved for
reference in case the landscape changes.

## Background

As of November 2025, Google's Quick Share on Android supports AirDrop
interoperability (driven by EU Digital Markets Act). Pixel 10 shipped first,
broader OEM rollout happening in 2026. This means Android phones can already
send files to iPhones via AirDrop - and receive from them.

However, there is no Quick Share app for iOS. The only way to receive files
from an iPhone on Linux is to speak AirDrop natively.

## The AirDrop Protocol

AirDrop is simpler than expected at the application layer. Three HTTPS POST
endpoints over IPv6 with binary plist bodies:

### Endpoints

| Endpoint    | Purpose                    | Request Body                        | Response Body                    |
|-------------|----------------------------|-------------------------------------|----------------------------------|
| `/Discover` | Capability check           | `SenderRecordData` (optional)       | Receiver name, model, capabilities |
| `/Ask`      | Request permission to send | Sender info, file metadata, icon    | Receiver name + model (accept) or HTTP error (decline) |
| `/Upload`   | Transfer files             | gzip-compressed CPIO archive        | HTTP 200 empty (success)         |

### Service Discovery

- DNS-SD service type: `_airdrop._tcp.local.`
- Service name: 12-character random hex ID
- TXT record: `flags=136` (0x88 = SUPPORTS_MIXED_TYPES | SUPPORTS_DISCOVER_MAYBE)
- Port: 8771 (auto-increment if busy)
- IPv6 only, link-local addresses

### TLS

- Self-signed RSA 2048-bit certificates
- No certificate verification required (both sides accept self-signed)
- Apple Root CA exists but OpenDrop skips validation entirely

### File Transfer Format

- Files wrapped in gzip-compressed CPIO archive
- HTTP chunked transfer encoding
- File types identified by Apple UTI strings (e.g. `public.image`)
- Optional JPEG2000 thumbnail in `/Ask` request

### Key Protocol Details

- `BundleID` is always `com.apple.finder`
- `SenderModelName` can be anything (OpenDrop uses "OpenDrop")
- Media capabilities is JSON encoded as bytes inside the plist
- `SenderRecordData` / `ReceiverRecordData` are Apple ID validation records (optional, can be omitted)

## Transport: AWDL (The Hard Part)

AirDrop runs over AWDL (Apple Wireless Direct Link) - a proprietary Wi-Fi
protocol. This is the main engineering challenge.

### What AWDL Is

A peer-to-peer Wi-Fi protocol where devices hop between channels (6, 44, 149)
on a synchronized schedule, exchanging data in time slots. It creates a
direct link without requiring a shared Wi-Fi network.

### Hardware Requirements

**Mandatory:** Wi-Fi card with active monitor mode and frame injection support.

Known compatible: Atheros AR9280 (802.11n). Most consumer laptop Wi-Fi cards
do NOT support this. There is no software workaround - this is a hardware/driver
limitation.

Detection at runtime:
```bash
iw list | grep -A 20 "Supported interface modes" | grep "monitor"
```
In Rust: query `NL80211_IFTYPE_MONITOR` via netlink (`neli` + `neli-wifi` crates).

### Platform Support

**Linux only.** Windows lacks the low-level Wi-Fi APIs needed for raw 802.11
frame injection from userspace. This is likely why Google's own AirDrop interop
is Android-only (their Windows Quick Share app doesn't support it).

## BLE Trigger

iPhones only activate their AWDL interface after receiving a specific BLE
advertisement. Without this trigger, the iPhone won't be discoverable even
if AirDrop is set to "Everyone".

The `apple-ble` crate can generate these advertisements via BlueZ on Linux.

## Existing Implementations

### OpenDrop (Python) - Reference Implementation
- GitHub: seemoo-lab/opendrop (~9.5k stars)
- Status: Functional, research-grade, maintained through 2024
- Implements full application protocol (Discover/Ask/Upload)
- Requires OWL for AWDL on Linux
- Does NOT implement BLE triggering
- No Apple ID verification (auto-accepts)
- Best source for protocol details

### OWL (C) - AWDL Daemon
- GitHub: seemoo-lab/owl (~1.5k stars)
- Creates virtual `awdl0` TAP interface on Linux
- Applications use it as a normal IPv6 network interface
- Dependencies: libpcap, libev, libnl3
- Requires dedicated Wi-Fi card with monitor mode + injection

### Rust AWDL Ecosystem (Frostie314159)

| Crate               | Purpose                        | Maturity     | Notes |
|----------------------|--------------------------------|-------------|-------|
| `awdl-frame-parser`  | AWDL frame/TLV parsing         | Production  | v0.4.1, no_std, 21k downloads |
| `grace`              | AWDL daemon in Rust            | Prototype   | v0.1.0, functional but incomplete election algorithm, no encryption, hardcoded interface names |
| `ieee80211-rs`       | 802.11 frame parsing           | Active      | 71 stars, dependency of grace |
| `apple-ble`          | Apple BLE advertisement gen    | Experimental | AirDrop ad type fully implemented, built on bluer/BlueZ |

Grace can establish AWDL peer connections and route IPv6 traffic today, but
is not production-ready (incomplete election, no security, panics on errors).

## Rust Crates for Application Protocol

All mature and ready to use:

| Component          | Crate       | Downloads | Notes |
|--------------------|-------------|-----------|-------|
| DNS-SD             | `mdns-sd`   | 1.9M      | Pure Rust, no system deps |
| HTTPS server       | `axum`      | -         | With `rustls` for TLS |
| Binary plist       | `plist`     | -         | Serde support, binary format |
| Self-signed certs  | `rcgen`     | -         | Certificate generation |
| CPIO archives      | `cpio`      | -         | Archive extraction |

## Architecture Recommendation

Two independent layers:

### Layer 1: AirDrop Application Protocol (Buildable Now)
- HTTPS server with `/Discover`, `/Ask`, `/Upload`
- DNS-SD service registration
- Binary plist serialization
- CPIO archive extraction
- Self-signed TLS certificate generation
- Testable on localhost without special hardware

### Layer 2: AWDL Transport (Requires Hardware)
- Option A: Shell out to OWL daemon, use `awdl0` as network interface
- Option B: Integrate grace as a library (needs stabilization work)
- BLE trigger via apple-ble
- Runtime detection of Wi-Fi card capabilities
- Graceful degradation when hardware unavailable

## Research Repos

Cloned to `research/` for reference:
- `research/opendrop/` - Protocol reference (Python)
- `research/owl/` - AWDL daemon (C)
- `research/grace/` - AWDL daemon (Rust)
- `research/awdl-frame-parser/` - Frame parsing (Rust)
- `research/apple-ble/` - BLE advertisements (Rust)

## References

- [Quick Share AirDrop interop announcement (9to5Google)](https://9to5google.com/2025/11/20/android-quick-share-airdrop-pixel-10/)
- [TechCrunch coverage](https://techcrunch.com/2025/11/20/androids-quick-share-now-works-with-iphones-airdrop-starting-with-the-pixel-10-lineup/)
- [USENIX Security 2019 - AWDL reverse engineering paper](https://www.usenix.org/conference/usenixsecurity19/presentation/stute)
- [OpenDrop GitHub](https://github.com/seemoo-lab/opendrop)
- [OWL GitHub](https://github.com/seemoo-lab/owl)
- [Grace GitHub](https://github.com/Frostie314159/grace)
