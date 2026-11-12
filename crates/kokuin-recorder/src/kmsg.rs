//! Keeps the whole kernel log while recording, before the kernel's ring buffer can
//! overwrite its first messages.

use std::{
    fs::File,
    io::{self, Read},
    sync::{Arc, Mutex, PoisonError},
    thread,
};

/// Reads `/dev/kmsg` in the background: every record since the start of the boot,
/// then each new one.
pub struct KmsgFollower {
    records: Arc<Mutex<Vec<u8>>>,
}

impl KmsgFollower {
    /// Opens `/dev/kmsg` and starts reading it.
    pub fn start() -> io::Result<Self> {
        let mut device = File::open("/dev/kmsg")?;
        let records = Arc::new(Mutex::new(Vec::new()));
        let shared = Arc::clone(&records);
        thread::Builder::new().name("kmsg".into()).spawn(move || {
            // Each read returns one record, and waits when there is none.
            let mut buffer = vec![0u8; 16 * 1024];
            loop {
                match device.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => shared
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .extend_from_slice(&buffer[..n]),
                    // Records were overwritten before being read: go on with the
                    // oldest one left.
                    Err(err) if err.kind() == io::ErrorKind::BrokenPipe => {}
                    Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
        })?;
        Ok(Self { records })
    }

    /// The records read so far, in the `/dev/kmsg` format.
    pub fn records(&self) -> Vec<u8> {
        self.records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}
