# Kvakk

Send and receive files with Android devices on your local network using Quick Share.

## How It Works

Kvakk is **always visible** to nearby devices on the same network. Unlike the official Quick Share apps which have visibility modes (Everyone, Contacts, Hidden), this app:

- **Always advertises** itself via mDNS on the local network
- **Anyone nearby** can see your device and send files to you
- **No contact restrictions** - works with any Quick Share compatible device
- **Manual acceptance required** - you must click Accept/Decline for incoming transfers

### Sending Files

1. Click on a discovered device
2. Select files to send
3. Wait for the recipient to accept

### Receiving Files

1. When someone sends files to you, an acceptance dialog appears
2. Review the sender and files, then Accept or Decline
3. Accepted files are saved to `~/Downloads` (or `~/Dropbox/Downloads` if it exists)

## Security Considerations

Because this app is always discoverable:

- Only run it on trusted networks (home, office)
- Be cautious on public WiFi - anyone nearby can see your device name
- Always verify the sender before accepting files

## Building

```bash
cargo build --release
```

The binary will be at `target/release/kvakk`.

## Requirements

- Desktop OS with GUI support (uses egui)
- Network access for mDNS discovery

## Related Projects

- [phoepsilonix/rquickshare](https://github.com/phoepsilonix/rquickshare) - This fork's origin
- [Martichou/rquickshare](https://github.com/Martichou/rquickshare) - Original Rust implementation
- [grishka/NearDrop](https://github.com/grishka/NearDrop) - Quick Share for macOS
