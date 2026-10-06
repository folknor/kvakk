//! Nearby Connections "weave" packet layer, used for the BLE data socket.
//!
//! A phone that discovered us over BLE (service 0xFEF3) opens a GATT connection
//! and talks to us through two characteristics: it writes weave packets to one
//! and we notify weave packets on the other. Every packet starts with a 1-byte
//! header:
//!
//! ```text
//!  bit  7   6 5 4    3       2      1 0
//!      | C |counter| first | last |  cmd  |   (cmd only when C = 1)
//! ```
//!
//! `C = 1` marks a control packet (connection request/confirm/error); data
//! packets carry a message fragment and are reassembled from first to last.
//! A reassembled message is `[service_id_hash(3)][data]`: hash `00 00 00`
//! marks a socket control frame (introduction/disconnection), the Nearby
//! Sharing hash `FC 9F 5E` marks data. Data is the same `[4-byte BE length]
//! [OfflineFrame]` stream the TCP (Wi-Fi LAN) path carries, so the inbound
//! state machine runs on top of it unchanged.
//!
//! Layout follows google/nearby `internal/weave/packet.cc` and the BLE socket
//! demux in its BLE medium.

/// 3-byte hash of the "NearbySharing" service id, same as in the mDNS name.
pub const NEARBY_SHARING_HASH: [u8; 3] = [0xfc, 0x9f, 0x5e];

const CONTROL_BIT: u8 = 0b1000_0000;
const COUNTER_SHIFT: u8 = 4;
const FIRST_BIT: u8 = 0b0000_1000;
const LAST_BIT: u8 = 0b0000_0100;
const CMD_MASK: u8 = 0b0000_1111;

const CMD_CONN_REQUEST: u8 = 0;
const CMD_CONN_CONFIRM: u8 = 1;
const CMD_ERROR: u8 = 2;

const PROTOCOL_VERSION: u16 = 1;
/// Packet size used when the connection request does not carry one.
const DEFAULT_PACKET_SIZE: u16 = 100;
const MIN_PACKET_SIZE: u16 = 20;
const MAX_PACKET_SIZE: u16 = 509;

/// SocketControlFrame type DISCONNECTION (INTRODUCTION is 1 and ignored).
const SOCKET_CTRL_DISCONNECTION: u8 = 2;

/// If `pkt` is a weave CONNECTION_REQUEST, return the packet size to select.
///
/// Request layout: `80 <min_ver(2)> <max_ver(2)> <max_packet_size(2)>`.
pub fn parse_conn_request(pkt: &[u8]) -> Option<u16> {
    let hdr = *pkt.first()?;
    if hdr & CONTROL_BIT == 0 || hdr & CMD_MASK != CMD_CONN_REQUEST {
        return None;
    }
    let size = match pkt.get(5..7) {
        Some(b) => u16::from_be_bytes([b[0], b[1]]),
        None => DEFAULT_PACKET_SIZE,
    };
    Some(size.clamp(MIN_PACKET_SIZE, MAX_PACKET_SIZE))
}

/// Build the CONNECTION_CONFIRM reply: `81 <version(2)> <packet_size(2)>`.
/// It is the first packet we send, so it uses counter 0.
pub fn conn_confirm(packet_size: u16) -> Vec<u8> {
    let mut pkt = vec![CONTROL_BIT | CMD_CONN_CONFIRM];
    pkt.extend_from_slice(&PROTOCOL_VERSION.to_be_bytes());
    pkt.extend_from_slice(&packet_size.to_be_bytes());
    pkt
}

/// What a received weave packet amounts to once reassembled.
#[derive(Debug, PartialEq, Eq)]
pub enum Incoming {
    /// Nothing to act on yet (a middle fragment, or an ignorable control packet).
    Pending,
    /// The peer sent a weave ERROR or a socket DISCONNECTION: close the session.
    Close,
    /// Socket control frame we do not act on (e.g. INTRODUCTION).
    Control(Vec<u8>),
    /// Data for the inbound stream, service hash already stripped.
    Data(Vec<u8>),
}

/// Reassembles weave data packets into BLE socket messages.
#[derive(Debug, Default)]
pub struct Reassembler {
    buf: Vec<u8>,
}

impl Reassembler {
    pub fn push(&mut self, pkt: &[u8]) -> Incoming {
        let Some((&hdr, body)) = pkt.split_first() else {
            return Incoming::Pending;
        };
        if hdr & CONTROL_BIT != 0 {
            return if hdr & CMD_MASK == CMD_ERROR {
                Incoming::Close
            } else {
                Incoming::Pending
            };
        }
        if hdr & FIRST_BIT != 0 {
            self.buf.clear();
        }
        self.buf.extend_from_slice(body);
        if hdr & LAST_BIT == 0 {
            return Incoming::Pending;
        }
        classify(std::mem::take(&mut self.buf))
    }
}

fn classify(msg: Vec<u8>) -> Incoming {
    match msg.get(0..3) {
        None => Incoming::Pending,
        Some([0, 0, 0]) => {
            // SocketControlFrame protobuf: field 1 (type) is `08 <type>`.
            if msg.get(3..5) == Some([0x08, SOCKET_CTRL_DISCONNECTION].as_slice()) {
                Incoming::Close
            } else {
                Incoming::Control(msg)
            }
        }
        Some(_) => Incoming::Data(msg[3..].to_vec()),
    }
}

/// Splits the inbound stream's output into complete `[len][frame]` entries.
#[derive(Debug, Default)]
pub struct EntrySplitter {
    buf: Vec<u8>,
}

impl EntrySplitter {
    /// Append bytes and return every complete `[4-byte len][frame]` entry.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.buf.extend_from_slice(bytes);
        let mut entries = Vec::new();
        while let Some(len_bytes) = self.buf.get(0..4) {
            let len = u32::from_be_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]]);
            let Ok(len) = usize::try_from(len) else {
                break;
            };
            if self.buf.len() < 4 + len {
                break;
            }
            entries.push(self.buf.drain(0..4 + len).collect());
        }
        entries
    }
}

/// Fragments outgoing BLE socket messages into weave data packets.
#[derive(Debug)]
pub struct Fragmenter {
    /// 3-bit packet counter; CONNECTION_CONFIRM used 0.
    counter: u8,
    /// Payload bytes per packet (selected packet size minus the header).
    max_payload: usize,
}

impl Fragmenter {
    pub fn new(packet_size: u16) -> Self {
        Self {
            counter: 1,
            max_payload: usize::from(packet_size.saturating_sub(1).max(MIN_PACKET_SIZE - 1)),
        }
    }

    /// Wrap one `[len][frame]` entry as a data message and split it into packets.
    pub fn fragment(&mut self, entry: &[u8]) -> Vec<Vec<u8>> {
        let mut message = Vec::with_capacity(3 + entry.len());
        message.extend_from_slice(&NEARBY_SHARING_HASH);
        message.extend_from_slice(entry);

        let chunks: Vec<&[u8]> = message.chunks(self.max_payload).collect();
        let last_index = chunks.len().saturating_sub(1);
        let mut packets = Vec::with_capacity(chunks.len());
        for (i, chunk) in chunks.into_iter().enumerate() {
            let mut hdr = (self.counter & 0x07) << COUNTER_SHIFT;
            if i == 0 {
                hdr |= FIRST_BIT;
            }
            if i == last_index {
                hdr |= LAST_BIT;
            }
            let mut pkt = Vec::with_capacity(1 + chunk.len());
            pkt.push(hdr);
            pkt.extend_from_slice(chunk);
            packets.push(pkt);
            self.counter = self.counter.wrapping_add(1);
        }
        packets
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn conn_request_selects_packet_size() {
        assert_eq!(parse_conn_request(&[0x80, 0, 1, 0, 1, 0x01, 0xfd]), Some(509));
        assert_eq!(parse_conn_request(&[0x80, 0, 1, 0, 1, 0x02, 0x00]), Some(509));
        assert_eq!(parse_conn_request(&[0x80, 0, 1, 0, 1, 0x00, 0x05]), Some(20));
        assert_eq!(parse_conn_request(&[0x80, 0, 1]), Some(100));
        // Data packet and other control commands are not requests.
        assert_eq!(parse_conn_request(&[0x0c, 1, 2]), None);
        assert_eq!(parse_conn_request(&[0x81, 0, 1, 1, 0xfd]), None);
        assert_eq!(parse_conn_request(&[]), None);
    }

    #[test]
    fn conn_confirm_layout() {
        assert_eq!(conn_confirm(0x01fd), vec![0x81, 0x00, 0x01, 0x01, 0xfd]);
    }

    #[test]
    fn reassembles_data_across_packets() {
        let mut r = Reassembler::default();
        assert_eq!(r.push(&[0x08, 0xfc, 0x9f]), Incoming::Pending);
        assert_eq!(r.push(&[0x14, 0x5e, 0xaa, 0xbb]), Incoming::Data(vec![0xaa, 0xbb]));
        // Single-packet message.
        assert_eq!(r.push(&[0x2c, 0xfc, 0x9f, 0x5e, 0x01]), Incoming::Data(vec![0x01]));
    }

    #[test]
    fn first_bit_discards_stale_fragment() {
        let mut r = Reassembler::default();
        assert_eq!(r.push(&[0x08, 0xde, 0xad]), Incoming::Pending);
        assert_eq!(r.push(&[0x0c, 0xfc, 0x9f, 0x5e, 0x07]), Incoming::Data(vec![0x07]));
    }

    #[test]
    fn control_frames_and_errors() {
        let mut r = Reassembler::default();
        let intro = [0x0c, 0, 0, 0, 0x08, 0x01, 0x12, 0x00];
        assert!(matches!(r.push(&intro), Incoming::Control(_)));
        assert_eq!(r.push(&[0x0c, 0, 0, 0, 0x08, 0x02]), Incoming::Close);
        assert_eq!(r.push(&[0x82]), Incoming::Close);
        assert_eq!(r.push(&[0x81, 0, 1]), Incoming::Pending);
    }

    #[test]
    fn splitter_emits_complete_entries_only() {
        let mut s = EntrySplitter::default();
        assert!(s.push(&[0, 0, 0, 3, 1]).is_empty());
        let out = s.push(&[2, 3, 0, 0, 0, 1, 9, 0, 0]);
        assert_eq!(out, vec![vec![0, 0, 0, 3, 1, 2, 3], vec![0, 0, 0, 1, 9]]);
        assert_eq!(s.push(&[0, 0]), vec![vec![0, 0, 0, 0]]);
    }

    #[test]
    fn fragmenter_splits_and_counts() {
        let mut f = Fragmenter::new(20);
        let entry: Vec<u8> = (0..30).collect();
        let pkts = f.fragment(&entry);
        // 3 hash bytes + 30 entry bytes over 19-byte payloads = 2 packets.
        assert_eq!(pkts.len(), 2);
        assert_eq!(pkts[0][0], 0x18); // counter 1, first
        assert_eq!(&pkts[0][1..4], &NEARBY_SHARING_HASH);
        assert_eq!(pkts[1][0], 0x24); // counter 2, last
        let rebuilt: Vec<u8> = pkts.iter().flat_map(|p| p[1..].to_vec()).collect();
        assert_eq!(&rebuilt[3..], entry.as_slice());

        // Round-trips through the reassembler.
        let mut r = Reassembler::default();
        assert_eq!(r.push(&pkts[0]), Incoming::Pending);
        assert_eq!(r.push(&pkts[1]), Incoming::Data(entry));
    }

    #[test]
    fn fragmenter_counter_wraps_at_three_bits() {
        let mut f = Fragmenter::new(509);
        let mut headers = Vec::new();
        for _ in 0..8 {
            headers.push(f.fragment(&[1])[0][0]);
        }
        assert_eq!(headers[0], 0x1c);
        assert_eq!(headers[6], 0x7c);
        assert_eq!(headers[7], 0x0c);
    }
}
