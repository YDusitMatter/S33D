//! A dedicated loopback connection tracks the desktop process, not an API
//! request or the lifetime of a particular Agent Control listener.
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::Mutex;
use std::time::Duration;

pub(super) const WATCH_REQUEST: &[u8] = b"S33D-MCP-WATCH/1\n";
const WATCH_ACK: &[u8] = b"S33D-MCP-WATCHING/1\n";
const MAX_WATCHES: usize = 64;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_millis(500);

pub struct DesktopLifetime {
    watchers: Mutex<Option<Vec<TcpStream>>>,
}

impl Default for DesktopLifetime {
    fn default() -> Self {
        Self {
            watchers: Mutex::new(Some(Vec::new())),
        }
    }
}

impl DesktopLifetime {
    /// Called with a nonblocking socket. A watcher has no application traffic
    /// after the acknowledgement, so the desktop only needs to hold the socket.
    pub(super) fn watch(&self, mut stream: TcpStream) {
        let mut state = self.watchers.lock().unwrap();
        let Some(watchers) = state.as_mut() else {
            return;
        };
        watchers.retain(|peer| {
            matches!(peer.peek(&mut [0]), Err(error) if error.kind() == io::ErrorKind::WouldBlock)
        });
        if watchers.len() < MAX_WATCHES && stream.write_all(WATCH_ACK).is_ok() {
            watchers.push(stream);
        }
    }

    pub fn close(&self) {
        if let Some(watchers) = self.watchers.lock().unwrap().take() {
            for stream in watchers {
                let _ = stream.shutdown(Shutdown::Both);
            }
        }
    }
}

#[allow(dead_code)] // Used by the bridge binary, which shares this module.
pub fn connect_desktop_watch(address: SocketAddr) -> io::Result<TcpStream> {
    if !address.ip().is_loopback() {
        return Err(io::ErrorKind::Unsupported.into());
    }
    let mut stream = TcpStream::connect_timeout(&address, HANDSHAKE_TIMEOUT)?;
    stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
    stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT))?;
    stream.write_all(WATCH_REQUEST)?;
    let mut acknowledgement = [0; WATCH_ACK.len()];
    stream.read_exact(&mut acknowledgement)?;
    if acknowledgement != WATCH_ACK {
        // An older desktop may return its usual JSON error for the frame.
        return Err(io::ErrorKind::Unsupported.into());
    }
    stream.set_read_timeout(None)?;
    Ok(stream)
}
