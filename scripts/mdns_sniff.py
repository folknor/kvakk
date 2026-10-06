#!/usr/bin/env python3
"""Print every IPv4 mDNS packet on the LAN: source, questions and records.

Usage: python3 -I scripts/mdns_sniff.py [substring-filter ...]

Shares port 5353 with avahi/kvakk via SO_REUSEPORT. If filters are given,
only packets whose source or any name contains one of them are printed.
"""

import socket
import struct
import sys
import time

TYPES = {1: "A", 12: "PTR", 16: "TXT", 28: "AAAA", 33: "SRV", 47: "NSEC", 255: "ANY"}


def read_name(data, off):
    labels = []
    jumped = False
    end = off
    for _ in range(128):
        if off >= len(data):
            break
        ln = data[off]
        if ln == 0:
            off += 1
            break
        if ln & 0xC0 == 0xC0:
            ptr = ((ln & 0x3F) << 8) | data[off + 1]
            if not jumped:
                end = off + 2
            jumped = True
            off = ptr
            continue
        labels.append(data[off + 1:off + 1 + ln].decode("utf-8", "replace"))
        off += 1 + ln
    if not jumped:
        end = off
    return ".".join(labels), end


def parse(data):
    _id, flags, qd, an, ns, ar = struct.unpack("!HHHHHH", data[:12])
    off = 12
    out = []
    for _ in range(qd):
        name, off = read_name(data, off)
        qtype, qclass = struct.unpack("!HH", data[off:off + 4])
        off += 4
        unicast = " QU" if qclass & 0x8000 else ""
        out.append(f"  Q  {TYPES.get(qtype, qtype):<5} {name}{unicast}")
    for section, count in (("AN", an), ("NS", ns), ("AR", ar)):
        for _ in range(count):
            name, off = read_name(data, off)
            rtype, _rclass, ttl, rdlen = struct.unpack("!HHIH", data[off:off + 10])
            off += 10
            rdata = data[off:off + rdlen]
            detail = ""
            if rtype == 12:
                detail = read_name(data, off)[0]
            elif rtype == 33:
                port = struct.unpack("!H", rdata[4:6])[0]
                detail = f"{read_name(data, off + 6)[0]}:{port}"
            elif rtype == 1:
                detail = socket.inet_ntoa(rdata)
            elif rtype == 16:
                parts = []
                i = 0
                while i < len(rdata):
                    ln = rdata[i]
                    parts.append(rdata[i + 1:i + 1 + ln].decode("utf-8", "replace"))
                    i += 1 + ln
                detail = " ".join(parts)
            off += rdlen
            out.append(f"  {section} {TYPES.get(rtype, rtype):<5} {name} ttl={ttl} {detail}")
    kind = "response" if flags & 0x8000 else "query"
    return kind, out


def main():
    filters = sys.argv[1:]
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)
    sock.bind(("", 5353))
    mreq = struct.pack("4s4s", socket.inet_aton("224.0.0.251"), socket.inet_aton("0.0.0.0"))
    sock.setsockopt(socket.IPPROTO_IP, socket.IP_ADD_MEMBERSHIP, mreq)
    print("listening on 224.0.0.251:5353", flush=True)
    while True:
        data, src = sock.recvfrom(9000)
        try:
            kind, lines = parse(data)
        except (struct.error, IndexError) as e:
            kind, lines = "unparsable", [f"  {e}"]
        text = "\n".join(lines)
        if filters and not any(f in src[0] or f in text for f in filters):
            continue
        stamp = time.strftime("%H:%M:%S")
        print(f"{stamp} {src[0]}:{src[1]} {kind}\n{text}", flush=True)


if __name__ == "__main__":
    main()
