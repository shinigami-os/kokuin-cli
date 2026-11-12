//! The kernel's process events connector (`CONFIG_PROC_EVENTS`).
//!
//! A program subscribes through a netlink socket of the connector family. The kernel
//! then sends one message per process event: creation (fork), new program (exec),
//! exit, with the time (`CLOCK_MONOTONIC`, the boot clock).
//!
//! Messages are a netlink header, a connector header, then a `proc_event`, all in the
//! machine's byte order (`linux/netlink.h`, `linux/connector.h`, `linux/cn_proc.h`).

use std::{io, time::Duration};

use rustix::{
    fd::OwnedFd,
    io::Errno,
    net::{
        AddressFamily, RecvFlags, SendFlags, SocketType, bind, netlink, recv, sendto,
        sockopt::{self, Timeout},
    },
};

const NLMSG_HEADER_LEN: usize = 16;
const NLMSG_NOOP: u16 = 1;
const NLMSG_ERROR: u16 = 2;
const NLMSG_DONE: u16 = 3;
const CN_MSG_LEN: usize = 20;
/// Connector channel of process events.
const CN_IDX_PROC: u32 = 1;
const CN_VAL_PROC: u32 = 1;
const PROC_CN_MCAST_LISTEN: u32 = 1;

const PROC_EVENT_NONE: u32 = 0;
const PROC_EVENT_FORK: u32 = 0x1;
const PROC_EVENT_EXEC: u32 = 0x2;
const PROC_EVENT_EXIT: u32 = 0x8000_0000;

/// A process event, as the kernel reports it. Threads are reported too: a process
/// is a thread group, whose id (`tgid`) is the PID users see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// Answer to the subscription; `err` is 0 when it worked.
    Ack {
        /// Error number.
        err: u32,
    },
    /// A process or thread was created.
    Fork {
        /// Boot clock, in ns.
        t_ns: i64,
        /// Process of the creator.
        parent_tgid: u32,
        /// Id of the new thread.
        child_pid: u32,
        /// Process of the new thread (equal to `child_pid` for a new process).
        child_tgid: u32,
    },
    /// A process started another program.
    Exec {
        /// Boot clock, in ns.
        t_ns: i64,
        /// The process.
        tgid: u32,
    },
    /// A thread ended (the process ends with its main thread, `pid == tgid`).
    Exit {
        /// Boot clock, in ns.
        t_ns: i64,
        /// Id of the thread.
        pid: u32,
        /// Its process.
        tgid: u32,
        /// Wait status: exit code or signal.
        status: u32,
    },
    /// An event kokuin does not use (uid change, session, ptrace...).
    Other,
}

/// Splits a wait status into the exit code, or the signal that killed the process.
pub fn exit_status(status: u32) -> (Option<i32>, Option<i32>) {
    let signal = (status & 0x7f) as i32;
    if signal == 0 {
        (Some(((status >> 8) & 0xff) as i32), None)
    } else {
        (None, Some(signal))
    }
}

/// The message subscribing to process events.
pub fn subscription() -> Vec<u8> {
    let payload_len = 4;
    let total = NLMSG_HEADER_LEN + CN_MSG_LEN + payload_len;
    let mut message = Vec::with_capacity(total);
    // Netlink header: length, type, flags, sequence, port (0: the kernel picks it).
    message.extend((total as u32).to_ne_bytes());
    message.extend(NLMSG_DONE.to_ne_bytes());
    message.extend(0u16.to_ne_bytes());
    message.extend(0u32.to_ne_bytes());
    message.extend(0u32.to_ne_bytes());
    // Connector header: channel, sequence, ack, payload length, flags.
    message.extend(CN_IDX_PROC.to_ne_bytes());
    message.extend(CN_VAL_PROC.to_ne_bytes());
    message.extend(0u32.to_ne_bytes());
    message.extend(0u32.to_ne_bytes());
    message.extend((payload_len as u16).to_ne_bytes());
    message.extend(0u16.to_ne_bytes());
    message.extend(PROC_CN_MCAST_LISTEN.to_ne_bytes());
    message
}

/// Decodes the events of one datagram (it may hold several netlink messages).
pub fn decode(datagram: &[u8]) -> Vec<Event> {
    let mut events = Vec::new();
    let mut offset = 0;
    while let Some(header) = datagram.get(offset..offset + NLMSG_HEADER_LEN) {
        let len = u32_at(header, 0).unwrap_or(0) as usize;
        let kind = u16_at(header, 4).unwrap_or(0);
        if len < NLMSG_HEADER_LEN {
            break;
        }
        let Some(message) = datagram.get(offset..offset + len) else {
            break;
        };
        if !matches!(kind, NLMSG_NOOP | NLMSG_ERROR) {
            if let Some(event) = message
                .get(NLMSG_HEADER_LEN + CN_MSG_LEN..)
                .and_then(decode_event)
            {
                events.push(event);
            }
        }
        // Messages are aligned on 4 bytes.
        offset += (len + 3) & !3;
    }
    events
}

/// Decodes a `proc_event`: what, cpu, timestamp, then the data of that kind.
fn decode_event(event: &[u8]) -> Option<Event> {
    let what = u32_at(event, 0)?;
    let t_ns = i64::try_from(u64_at(event, 8)?).ok()?;
    let data = event.get(16..)?;
    Some(match what {
        PROC_EVENT_NONE => Event::Ack {
            err: u32_at(data, 0)?,
        },
        PROC_EVENT_FORK => Event::Fork {
            t_ns,
            parent_tgid: u32_at(data, 4)?,
            child_pid: u32_at(data, 8)?,
            child_tgid: u32_at(data, 12)?,
        },
        PROC_EVENT_EXEC => Event::Exec {
            t_ns,
            tgid: u32_at(data, 4)?,
        },
        PROC_EVENT_EXIT => Event::Exit {
            t_ns,
            pid: u32_at(data, 0)?,
            tgid: u32_at(data, 4)?,
            status: u32_at(data, 8)?,
        },
        _ => Event::Other,
    })
}

fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_ne_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn u64_at(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_ne_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

/// A subscription to process events.
pub struct Listener {
    socket: OwnedFd,
    buffer: Vec<u8>,
}

/// What [`Listener::receive`] got.
pub enum Received {
    /// Some events.
    Events(Vec<Event>),
    /// Nothing during the wait.
    Nothing,
    /// The kernel dropped events: the socket buffer was full.
    Lost,
}

impl Listener {
    /// Subscribes, and waits up to `confirm` for the kernel to answer. Without an
    /// answer, the kernel has no process events (`CONFIG_PROC_EVENTS`).
    pub fn subscribe(confirm: Duration) -> io::Result<Self> {
        let socket = rustix::net::socket(
            AddressFamily::NETLINK,
            SocketType::DGRAM,
            Some(netlink::CONNECTOR),
        )?;
        bind(&socket, &netlink::SocketAddrNetlink::new(0, CN_IDX_PROC))?;
        // Room for bursts: hundreds of processes can start in a few milliseconds.
        let _ = sockopt::set_socket_recv_buffer_size(&socket, 4 << 20);
        sendto(
            &socket,
            &subscription(),
            SendFlags::empty(),
            &netlink::SocketAddrNetlink::new(0, 0),
        )?;
        let mut listener = Self {
            socket,
            buffer: vec![0; 64 * 1024],
        };
        listener.set_wait(confirm)?;
        loop {
            match listener.receive()? {
                Received::Events(events) => {
                    match events.iter().find_map(|e| match e {
                        Event::Ack { err } => Some(*err),
                        _ => None,
                    }) {
                        Some(0) => return Ok(listener),
                        Some(err) => return Err(io::Error::from_raw_os_error(err as i32)),
                        None => {}
                    }
                }
                Received::Nothing => {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "the kernel did not confirm the subscription to process events",
                    ));
                }
                Received::Lost => {}
            }
        }
    }

    /// How long [`Listener::receive`] waits for events.
    pub fn set_wait(&mut self, wait: Duration) -> io::Result<()> {
        sockopt::set_socket_timeout(&self.socket, Timeout::Recv, Some(wait))?;
        Ok(())
    }

    /// Waits for the next events.
    pub fn receive(&mut self) -> io::Result<Received> {
        match recv(&self.socket, &mut self.buffer[..], RecvFlags::empty()) {
            Ok((len, _)) => Ok(Received::Events(decode(&self.buffer[..len]))),
            Err(Errno::AGAIN | Errno::INTR) => Ok(Received::Nothing),
            Err(Errno::NOBUFS) => Ok(Received::Lost),
            Err(err) => Err(err.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A datagram as the kernel sends it: netlink and connector headers, then the
    /// event.
    fn datagram(what: u32, t_ns: u64, data: &[u32]) -> Vec<u8> {
        let mut event = Vec::new();
        event.extend(what.to_ne_bytes());
        event.extend(0u32.to_ne_bytes());
        event.extend(t_ns.to_ne_bytes());
        for value in data {
            event.extend(value.to_ne_bytes());
        }
        let total = NLMSG_HEADER_LEN + CN_MSG_LEN + event.len();
        let mut message = Vec::new();
        message.extend((total as u32).to_ne_bytes());
        message.extend(NLMSG_DONE.to_ne_bytes());
        message.extend([0u8; 10]);
        message.extend([0u8; CN_MSG_LEN]);
        message.extend(event);
        message
    }

    #[test]
    fn the_subscription_is_a_netlink_connector_message() {
        let message = subscription();
        assert_eq!(message.len(), 40);
        assert_eq!(u32_at(&message, 0), Some(40));
        assert_eq!(u32_at(&message, NLMSG_HEADER_LEN), Some(CN_IDX_PROC));
        assert_eq!(u32_at(&message, 36), Some(PROC_CN_MCAST_LISTEN));
    }

    #[test]
    fn decodes_fork_exec_exit_and_ack() {
        let mut bytes = datagram(PROC_EVENT_FORK, 1_500, &[1, 1, 300, 300]);
        bytes.extend(datagram(PROC_EVENT_EXEC, 1_600, &[300, 300]));
        bytes.extend(datagram(PROC_EVENT_EXIT, 1_700, &[300, 300, 9, 17, 1, 1]));
        bytes.extend(datagram(PROC_EVENT_NONE, 0, &[0]));
        bytes.extend(datagram(0x200, 1_800, &[300, 300]));
        assert_eq!(
            decode(&bytes),
            [
                Event::Fork {
                    t_ns: 1_500,
                    parent_tgid: 1,
                    child_pid: 300,
                    child_tgid: 300
                },
                Event::Exec {
                    t_ns: 1_600,
                    tgid: 300
                },
                Event::Exit {
                    t_ns: 1_700,
                    pid: 300,
                    tgid: 300,
                    status: 9
                },
                Event::Ack { err: 0 },
                Event::Other,
            ]
        );
    }

    #[test]
    fn ignores_a_truncated_message() {
        let mut bytes = datagram(PROC_EVENT_EXEC, 1_600, &[300, 300]);
        bytes.truncate(30);
        assert!(decode(&bytes).is_empty());
    }

    #[test]
    fn splits_wait_statuses() {
        assert_eq!(exit_status(0), (Some(0), None));
        assert_eq!(exit_status(1 << 8), (Some(1), None));
        assert_eq!(exit_status(9), (None, Some(9)));
    }
}
