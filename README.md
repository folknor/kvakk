# Kvakk

Send and receive files with nearby devices on your local network using Quick Share and LocalSend.

- **Quick Share** — Android devices
- **LocalSend** — iOS, macOS, Windows, Linux, and anything running the LocalSend app

Runs on Linux and Windows.

Built with LLMs. See [LLM.md](LLM.md).


## How It Works

Kvakk is **always visible** to nearby devices on the same network. Unlike the official Quick Share apps which have visibility modes (Everyone, Contacts, Hidden), this app:

- **Always advertises** itself via mDNS, BLE, and LocalSend multicast
- **Anyone nearby** can see your device and send files to you
- **No contact restrictions** - works with any compatible device
- **Auto-accepts** all incoming transfers — no confirmation dialogs
- **Unified device grid** — Quick Share and LocalSend devices appear side by side

### Sending Files

1. Click on a discovered device (Quick Share or LocalSend)
2. Select files to send
3. Wait for the recipient to accept

### Receiving Files

Files are automatically accepted and saved to your Downloads folder.

## Security Considerations

Because this app is always discoverable:

- Only run it on trusted networks (home, office)
- Be cautious on public WiFi - anyone nearby can see your device name

## Building

```bash
cargo build --release
```

## Requirements

- Desktop OS with GUI support (uses egui)
- Network access for mDNS discovery
- Bluetooth adapter (optional, for BLE discovery)

## Related Projects

- [phoepsilonix/rquickshare](https://github.com/phoepsilonix/rquickshare) - This fork's origin
- [Martichou/rquickshare](https://github.com/Martichou/rquickshare) - Original Rust implementation
- [grishka/NearDrop](https://github.com/grishka/NearDrop) - Quick Share for macOS
- [localsend/localsend](https://github.com/localsend/localsend) - LocalSend official app
- [CrossCopy/localsend-rs](https://github.com/CrossCopy/localsend-rs) - Rust LocalSend library, vendored into `src/rqs/localsend/`

## Acknowledgements

The LocalSend implementation under `src/rqs/localsend/` was initially
vendored from [CrossCopy/localsend-rs](https://github.com/CrossCopy/localsend-rs)
and then trimmed and refactored to suit kvakk's needs. That initial code
contribution is licensed under the MIT license; the rest of kvakk is
licensed under AGPL-3.0.
