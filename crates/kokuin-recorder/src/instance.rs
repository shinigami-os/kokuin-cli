//! One recorder at a time.
//!
//! A boot can start the recorder twice: from the initramfs, then from kokuin-init. The
//! recorder holds the abstract Unix socket [`NAME`] while it runs: abstract sockets
//! have no file, so the name stays reserved across the switch to the real root. Who
//! cannot take the name knows a recorder is running.

use std::{
    io,
    os::{
        linux::net::SocketAddrExt,
        unix::net::{SocketAddr, UnixListener},
    },
};

/// The abstract socket name (`@kokuin-recorder`). kokuin-init uses the same.
pub const NAME: &[u8] = b"kokuin-recorder";

/// The name, held as long as the value lives.
pub struct Claim {
    _socket: UnixListener,
}

/// Takes the name; fails with `AddrInUse` when a recorder is already running.
pub fn claim() -> io::Result<Claim> {
    let address = SocketAddr::from_abstract_name(NAME)?;
    Ok(Claim {
        _socket: UnixListener::bind_addr(&address)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_claim_fails_while_the_first_lives() {
        let first = claim().unwrap();
        assert_eq!(
            claim().err().map(|e| e.kind()),
            Some(io::ErrorKind::AddrInUse)
        );
        drop(first);
        assert!(claim().is_ok());
    }
}
