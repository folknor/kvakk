use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, DuplexStream, ReadBuf};
use tokio::net::TcpStream;

/// Transport for a BLE-initiated inbound transfer. It starts on the BLE weave
/// socket and is swapped to TCP by the Wi-Fi LAN bandwidth upgrade. Only the
/// socket changes; `InboundRequest` keeps its keys and sequence numbers.
#[derive(Debug)]
pub enum MigratableStream {
    /// BLE weave data socket (one end of an in-memory duplex).
    Ble(DuplexStream),
    /// Wi-Fi LAN TCP socket after a bandwidth upgrade.
    Tcp(TcpStream),
}

impl AsyncRead for MigratableStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MigratableStream::Ble(s) => Pin::new(s).poll_read(cx, buf),
            MigratableStream::Tcp(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for MigratableStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            MigratableStream::Ble(s) => Pin::new(s).poll_write(cx, buf),
            MigratableStream::Tcp(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MigratableStream::Ble(s) => Pin::new(s).poll_flush(cx),
            MigratableStream::Tcp(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MigratableStream::Ble(s) => Pin::new(s).poll_shutdown(cx),
            MigratableStream::Tcp(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}
