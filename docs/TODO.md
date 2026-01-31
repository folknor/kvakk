# TODO

## Protocol Issues

### Unhandled Frame Type 12

**Status:** Needs investigation

**Observed:** 2026-01-31

**Log:**
```
[ERROR rqs::hdl::inbound] Unhandled offline frame encrypted: OfflineFrame {
  version: Some(V1),
  v1: Some(V1Frame {
    r#type: Some(12),
    connection_request: None,
    connection_response: None,
    payload_transfer: None,
    bandwidth_upgrade_negotiation: None,
    keep_alive: None,
    disconnection: None,
    paired_key_encryption: None
  })
}
```

**Analysis:**
- Frame type 12 is not defined in `offline_wire_formats.proto`
- Current known frame types (0-7):
  - 0: UNKNOWN_FRAME_TYPE
  - 1: CONNECTION_REQUEST
  - 2: CONNECTION_RESPONSE
  - 3: PAYLOAD_TRANSFER
  - 4: BANDWIDTH_UPGRADE_NEGOTIATION
  - 5: KEEP_ALIVE
  - 6: DISCONNECTION
  - 7: PAIRED_KEY_ENCRYPTION
- Google likely added new frame types to the Quick Share protocol

**Impact:** Transfer still succeeded despite this error - appears to be non-critical

**Next steps:**
1. Monitor if this causes any actual transfer failures
2. Check Google's nearby-connections repository for updated proto files
3. Potentially add handling or at least downgrade from ERROR to WARN if non-critical
