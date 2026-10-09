//! Networked lockstep for Parcels.
//!
//! Only commands cross the wire, never the world. The host owns the tick clock:
//! clients send it raw command requests, the host stamps each with author, tick and
//! sequence, steps its own authoritative simulation, then broadcasts the exact
//! ordered list it applied. Clients apply the same list on the same tick and so
//! compute the same world. A joining client gets one `GameState` snapshot first.
//!
//! Every `HASH_INTERVAL` ticks clients report a state hash; a mismatch means a
//! determinism bug, and the host answers with a fresh snapshot.
//!
//! No Bevy here: the headless `parcels-server` binary and the game's host mode
//! share this code.

pub mod client;
pub mod server;

use serde::{Deserialize, Serialize};

use parcels_sim::{Command, CommandKind, PlayerId};

pub use client::NetClient;
pub use server::{HostServer, ServerOptions};

/// Bump when the wire format or simulation rules change incompatibly.
pub const PROTOCOL_ID: u64 = 0x5041_5243_454c_0002;
pub const DEFAULT_PORT: u16 = 5757;
pub const HASH_INTERVAL: u64 = 30;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientMsg {
    Hello { name: String },
    /// A command request; the host decides author, tick and order.
    Submit(CommandKind),
    /// Hash of the state after `tick` ticks.
    Hash { tick: u64, hash: u64 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LobbyEntry {
    pub name: String,
    pub is_host: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerMsg {
    Lobby { entries: Vec<LobbyEntry>, slots: u8 },
    /// Full snapshot. Afterwards the client receives `Tick` from `snapshot.tick` on.
    Welcome { you: PlayerId, snapshot: Vec<u8> },
    /// The commands the host applied on `tick`, in order. Sent every tick, even empty.
    Tick { tick: u64, commands: Vec<Command> },
    Desync { tick: u64 },
    Notice(String),
}

pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    postcard::to_allocvec(msg).expect("message serializes")
}

pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Option<T> {
    postcard::from_bytes(bytes).ok()
}

/// Wall-clock time for the transport layer only. Never reaches the simulation.
pub(crate) fn now() -> std::time::Duration {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default()
}

/// This machine's address on the local network, for telling a friend where to
/// connect. Connecting a UDP socket sends nothing; it just picks the route.
pub fn lan_ip() -> Option<std::net::IpAddr> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:80").ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_unspecified()).then_some(ip)
}

/// Accept "host", "host:port", or a bare port, defaulting the rest.
pub fn parse_addr(s: &str) -> std::io::Result<std::net::SocketAddr> {
    use std::net::ToSocketAddrs;
    let s = s.trim();
    let full = if s.parse::<u16>().is_ok() {
        format!("127.0.0.1:{s}")
    } else if s.contains(':') {
        s.to_string()
    } else {
        format!("{s}:{DEFAULT_PORT}")
    };
    full.to_socket_addrs()?
        .find(|a| a.is_ipv4())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, format!("can't resolve {s}")))
}
