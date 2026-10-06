# Quick Share: phone drops off Wi-Fi, kvakk not discoverable

Investigation notes, 2026-10-06. Transient.

## Status

The BLE receiver path is ported into kvakk (Linux only): `ble_receiver.rs`,
`weave.rs`, `migratable.rs`, `inbound/bwu.rs`, spawned from `RQS::run`. It
builds, lints and its codec/advertisement unit tests pass, but it is **untested
end to end**: `dm6` has no Bluetooth adapter. Next steps:

- Plug in a USB Bluetooth adapter and send from the phone with the share sheet
  forcing it off Wi-Fi. Expect roughly 15-20 s before kvakk appears.
- Fix the 30 s mDNS announcement window (see below).
- Not ported from the same branch, unrelated to Bluetooth: MIME-based fix for
  generic received file extensions (`.tmp` -> `.pdf`), and an outbound change
  that waits for the receiver to close before finishing a send.
- Windows has no receiver advertiser yet (the asulwer fork has one).

Differences from the upstream branch: the TCP server's BWU peek stub is not
ported (`do_bwu` binds its own listener, so it is not needed); the weave session
ends when the phone stops its notify subscription, so a dropped phone cannot
wedge later sessions; the BWU path requires a real CLIENT_INTRODUCTION on the
upgraded socket; the offered IP uses the same interface filtering as mDNS.

## Symptom

Sending from the Android phone to kvakk over Wi-Fi stopped working: kvakk does
not appear in the phone's Quick Share target list. It used to work on the same
desktop host (`dm6`), which has never had Bluetooth.

## Root cause: Android, not kvakk

Opening the Quick Share share sheet makes the phone **disconnect from Wi-Fi**.
This is a known Android-side behaviour introduced with Quick Share's
AirDrop-compatibility update:

- Pixel 10 (and reports on Pixel 9 Pro, Pixel 11): since Quick Share extension
  1.0.815689706 / Play Services 25.45.x, November 2025. Wi-Fi list goes empty
  while the share sheet is open, returns when it is closed.
- Samsung Galaxy S26: documented by Samsung. With Settings > Connected devices >
  Quick Share > "Share with Apple devices" enabled, opening Quick Share drops
  5/6 GHz Wi-Fi.
- rquickshare issue #425 tracks the same failure for rquickshare.

While off Wi-Fi the phone can only discover receivers over Bluetooth. A
receiver that only speaks the Wi-Fi LAN medium (mDNS + TCP, which is all kvakk
does today) is invisible. The LocalSend work landing in the same period was a
coincidence.

Phone-side workarounds reported:

- Samsung: turn off "Share with Apple devices". Confirmed by two users on S26.
- Pixel: Settings > Apps > Quick Share extension > Uninstall updates (loses
  AirDrop interop; Play Store may reinstall).
- Enable hotspot, then Bluetooth, after opening the share sheet (fiddly).

## What we measured

Tooling: `scripts/mdns_sniff.py` (prints every IPv4 mDNS packet with decoded
questions and records, shares port 5353 via SO_REUSEPORT; filters are
substrings matched against source IP or record text).

Findings:

- The phone, while its share sheet is open, advertises
  `nearby-presence-nsd-<uuid>._nearbypresence._tcp.local` with a `bp=` TXT
  record (Nearby Presence). Each Wi-Fi rejoin gets a fresh random hostname
  (`Android_XXXXXXXX.local`), which is how the drop/rejoin shows up in captures.
- The phone sends exactly **one** `PTR _FC9F5ED42C8A._tcp.local` query per
  share-sheet opening, with the QU (unicast-response) bit set, from port 5353.
  It does not repeat it.
- kvakk (mdns-sd 0.21) answers that query correctly within a second: PTR in
  the answer section, SRV/TXT/A in additionals, multicast. mdns-sd handles a
  QU query from port 5353 as a delayed (10-50 ms) multicast response; only
  queries from a source port other than 5353 get a unicast legacy response.
- The phone still never lists kvakk and never opens the TCP connection. A
  commenter on #425 saw exactly the same with a Pixel 11: valid mDNS answer,
  no connection. The Bluetooth bootstrap is what matters.
- This host has no Bluetooth adapter (`/sys/class/bluetooth` missing,
  `bluetooth.service` skipped), so kvakk's BLE listener and advertiser fail
  after a 25 s D-Bus activation timeout. It also has no Wi-Fi card (only
  `enp9s0`, `lo`, `tailscale0`).

Ruled out: the dependency bumps since LocalSend landed (mdns-sd 0.17 -> 0.21,
p256 0.14, hkdf/hmac 0.13, sha2 0.11). Quick Share code only changed for API
churn. LocalSend's sockets (UDP 53317 multicast, HTTP 53317) do not touch
5353 or the Quick Share TCP port.

## Secondary weakness: mDNS announcement window

`MDnsServer` re-registers every 5 s for the first 30 s, then only when the BLE
listener sees a nearby Quick Share sender. Without Bluetooth that trigger never
fires. PTR/TXT records carry a 4500 s TTL but SRV/A only 120 s, so a phone that
cached kvakk early can hold a PTR whose SRV/A expired. The asulwer rquickshare
fork fixed the equivalent problem by re-announcing periodically while visible.
Not the cause of this bug, but worth fixing.

## Why not Wi-Fi Direct or Wi-Fi Aware

In `google/nearby` (`connections/implementation/p2p_cluster_pcp_handler.cc`)
the mediums a sender can discover and connect to a receiver over are Bluetooth
Classic, BLE, Wi-Fi LAN, Wi-Fi Aware and AWDL. Wi-Fi Direct and Wi-Fi hotspot
are bandwidth-upgrade mediums only: used after a connection already exists.

Wi-Fi Aware (NAN) needs firmware, kernel driver, cfg80211/mac80211 and a
userspace daemon. Devices in a NAN cluster wake in synchronised discovery
windows, which only firmware can keep. Linux status as of 2026:

- nl80211 has had basic NAN for years, but almost no drivers implemented it.
- Intel posted the NAN Data Path series for cfg80211/nl80211 (v1 Jan 2026 to
  v5 Mar 2026, new `NL80211_IFTYPE_NAN_DATA`), mac80211 NAN schedule/station
  support, `mac80211_hwsim` NAN emulation, and iwlwifi `mld` NAN data path
  patches through iwlwifi-next (May 2026). Some pieces reached 6.19 stable.
- Discovery and data path negotiation are expected to live in userspace
  (likely wpa_supplicant); status unknown.

Even with all that, nothing confirms the phone uses Wi-Fi Aware to discover
Quick Share receivers in the off-Wi-Fi mode. ChromeOS receivers are still
found, reportedly over Bluetooth. Bluetooth is the practical path.

## The fix: BLE receiver discovery

Implemented in martinalderson/rquickshare branch `feat/ble-receiver-connect-back`
(cloned to `research/rquickshare-martinalderson`, write-up in its
`docs/BLE_RECEIVER_DISCOVERY.md`). Confirmed working by a third party on a
Pixel 10 (three files received). Summary:

1. Receiver advertises service UUID `0xFEF3` with a Nearby `BleAdvertisement`
   carrying the same 4-byte endpoint ID used in the mDNS instance name.
2. Receiver runs a GATT server for `0xFEF3`: an advertisement read
   characteristic, a write characteristic (phone to us) and a notify
   characteristic (us to phone).
3. The "weave" packet layer runs over those characteristics; inside it, the
   byte stream is the same `[4-byte BE length][OfflineFrame]` framing as the
   TCP path, so the existing inbound state machine runs unchanged once
   `InboundRequest` is generic over `AsyncRead + AsyncWrite`.
4. After UKEY2, the receiver offers a bandwidth upgrade to Wi-Fi LAN
   (its IPv4 and TCP port); the phone reconnects over TCP and the payload
   streams at LAN speed with the same keys and sequence numbers.

Caveats from that branch and #425:

- The asulwer fork has a receiver advertiser too, but only on Windows.
- Ubuntu kernel 7.0.0-30 broke BlueZ extended advertising registration
  (Invalid Parameters 0x0d); fixed in 7.0.0-31.
- Discovery latency is roughly 15-20 s, mostly phone-side BLE and GATT
  discovery.
- Requires a Bluetooth adapter. `dm6` needs a USB dongle to test it.
