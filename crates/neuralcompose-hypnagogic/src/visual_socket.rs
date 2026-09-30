//! Socket ownership belongs to the shell, never the deterministic library.
use neuralcompose_hypnagogic::visual::VisualPublisher;
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};

pub struct Publisher {
    pub inner: VisualPublisher,
    path: PathBuf,
}
impl Publisher {
    pub fn publish(&self, state: &neuralcompose_hypnagogic::visual::TurnVisual) {
        self.inner.publish(state);
    }
}
impl Drop for Publisher {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(unix)]
pub fn start(path: &Path, session: String) -> io::Result<Publisher> {
    use std::os::unix::{fs::PermissionsExt, net::UnixListener};
    // Never remove an existing pathname: it may belong to another session.
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let (publisher, receiver) = VisualPublisher::new(session);
    std::thread::spawn(move || {
        let mut client = None;
        let mut sent = 0;
        loop {
            if client.is_none() {
                if let Ok((socket, _)) = listener.accept() {
                    let _ = socket.set_write_timeout(Some(Duration::from_millis(100)));
                    client = Some(socket);
                    sent = 0;
                }
            }
            if let (Some(socket), Some(packet)) = (client.as_mut(), receiver.latest()) {
                if packet.sequence != sent {
                    let bytes = serde_json::to_vec(packet.as_ref());
                    match bytes.and_then(|mut bytes| {
                        bytes.push(b'\n');
                        socket.write_all(&bytes).map_err(serde_json::Error::io)
                    }) {
                        Ok(()) => sent = packet.sequence,
                        Err(_) => client = None,
                    }
                }
            }
            if matches!(
                receiver.wake.recv_timeout(Duration::from_millis(50)),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
            ) {
                break;
            }
        }
    });
    Ok(Publisher {
        inner: publisher,
        path: path.to_path_buf(),
    })
}

#[cfg(not(unix))]
pub fn start(_: &Path, _: String) -> io::Result<Publisher> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "visualization IPC requires Unix",
    ))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use neuralcompose_hypnagogic::visual::{TurnVisual, VisualPacket};
    use std::{
        io::{BufRead, BufReader},
        os::unix::{fs::PermissionsExt, net::UnixStream},
    };
    #[test]
    fn socket_round_trip_is_private_and_does_not_replace_existing_paths() {
        let dir = std::env::temp_dir().join(format!("nc-visual-socket-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("feed.sock");
        let publisher = start(&path, "fixture-session".into()).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(start(&path, "collision".into()).is_err());
        let stream = UnixStream::connect(&path).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut state = TurnVisual::new(4);
        state.stage = "completed".into();
        publisher.publish(&state);
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).unwrap();
        let packet: VisualPacket = serde_json::from_str(&line).unwrap();
        assert_eq!(packet.session, "fixture-session");
        assert_eq!(packet.state, state);
        assert_eq!(packet.sequence, 1);
        drop(publisher);
        assert!(!path.exists());
        std::fs::remove_dir(dir).unwrap();
    }
}
