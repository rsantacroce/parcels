//! A lockstep client: mirrors the host's simulation by applying its command stream.

use std::collections::VecDeque;
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use renet::{ConnectionConfig, DefaultChannel, RenetClient};
use renet_netcode::{ClientAuthentication, NetcodeClientTransport};

use parcels_sim::{Command, CommandKind, GameState, PlayerId, TickReport};

use crate::{decode, encode, now, ClientMsg, LobbyEntry, ServerMsg, HASH_INTERVAL, PROTOCOL_ID};

pub struct NetClient {
    renet: RenetClient,
    transport: NetcodeClientTransport,
    name: String,
    said_hello: bool,
    pub state: Option<GameState>,
    pub me: Option<PlayerId>,
    queue: VecDeque<(u64, Vec<Command>)>,
    pub lobby: Vec<LobbyEntry>,
    pub slots: u8,
    pub log: Vec<String>,
    /// Number of resyncs received (each one means a determinism bug somewhere).
    pub desyncs: u32,
}

impl NetClient {
    pub fn connect(server: SocketAddr, name: &str) -> std::io::Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        let t = now();
        // Unique enough for a session among friends; never touches the sim.
        let client_id = t.as_nanos() as u64 ^ std::process::id() as u64;
        let auth = ClientAuthentication::Unsecure { protocol_id: PROTOCOL_ID, client_id, server_addr: server, user_data: None };
        let transport = NetcodeClientTransport::new(t, auth, socket).map_err(std::io::Error::other)?;
        Ok(Self {
            renet: RenetClient::new(ConnectionConfig::default()),
            transport,
            name: name.to_string(),
            said_hello: false,
            state: None,
            me: None,
            queue: VecDeque::new(),
            lobby: Vec::new(),
            slots: 0,
            log: Vec::new(),
            desyncs: 0,
        })
    }

    pub fn is_connected(&self) -> bool {
        self.renet.is_connected()
    }

    pub fn is_disconnected(&self) -> bool {
        self.renet.is_disconnected()
    }

    pub fn disconnect_reason(&self) -> Option<String> {
        self.renet.disconnect_reason().map(|r| r.to_string())
    }

    pub fn rtt_ms(&self) -> u32 {
        (self.renet.rtt() * 1000.0) as u32
    }

    fn send(&mut self, msg: &ClientMsg) {
        self.renet.send_message(DefaultChannel::ReliableOrdered, encode(msg));
    }

    /// Pump the network. Call every frame, then `flush`.
    pub fn poll(&mut self, dt: Duration) {
        self.renet.update(dt);
        if let Err(e) = self.transport.update(dt, &mut self.renet) {
            self.log.push(format!("network error: {e}"));
        }
        if self.renet.is_connected() && !self.said_hello {
            self.said_hello = true;
            let hello = ClientMsg::Hello { name: self.name.clone() };
            self.send(&hello);
        }
        while let Some(bytes) = self.renet.receive_message(DefaultChannel::ReliableOrdered) {
            match decode::<ServerMsg>(&bytes) {
                Some(ServerMsg::Lobby { entries, slots }) => {
                    self.lobby = entries;
                    self.slots = slots;
                }
                Some(ServerMsg::Welcome { you, snapshot }) => match GameState::from_bytes(&snapshot) {
                    Ok(s) => {
                        self.log.push(format!("Joined at tick {} as {}", s.tick, s.players[you.index()].name));
                        self.state = Some(s);
                        self.me = Some(you);
                        self.queue.clear();
                    }
                    Err(e) => self.log.push(format!("bad snapshot: {e}")),
                },
                Some(ServerMsg::Tick { tick, commands }) => self.queue.push_back((tick, commands)),
                Some(ServerMsg::Desync { tick }) => {
                    self.desyncs += 1;
                    self.log.push(format!("Desync detected at tick {tick}; resyncing from host"));
                }
                Some(ServerMsg::Notice(s)) => self.log.push(s),
                None => self.log.push("host sent garbage".into()),
            }
        }
    }

    pub fn flush(&mut self) {
        if let Err(e) = self.transport.send_packets(&mut self.renet) {
            self.log.push(format!("send error: {e}"));
        }
    }

    pub fn submit(&mut self, kind: CommandKind) {
        self.send(&ClientMsg::Submit(kind));
    }

    /// Ticks received but not yet applied.
    pub fn backlog(&self) -> usize {
        self.queue.len()
    }

    /// Apply the next tick if the host has sent it. Lockstep: never runs ahead.
    pub fn step(&mut self) -> Option<TickReport> {
        let state = self.state.as_mut()?;
        // Drop anything older than our snapshot.
        while self.queue.front().is_some_and(|(t, _)| *t < state.tick) {
            self.queue.pop_front();
        }
        let (tick, _) = self.queue.front()?;
        if *tick != state.tick {
            return None;
        }
        let (_, commands) = self.queue.pop_front()?;
        let report = state.step(&commands);
        if state.tick % HASH_INTERVAL == 0 {
            let msg = ClientMsg::Hash { tick: state.tick, hash: state.hash() };
            self.send(&msg);
        }
        Some(report)
    }

    pub fn disconnect(&mut self) {
        self.transport.disconnect();
    }
}
