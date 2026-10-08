//! The authoritative host: owns the tick clock and the canonical command order.

use std::collections::{BTreeMap, VecDeque};
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use renet::{ClientId, ConnectionConfig, DefaultChannel, RenetServer, ServerEvent};
use renet_netcode::{NetcodeServerTransport, ServerAuthentication, ServerConfig};

use parcels_sim::state::ai_name;
use parcels_sim::{
    AiStrategy, CommandKind, Config, Controller, GameState, NewGame, PlayerId, PlayerSetup, Session, TickReport,
};

use crate::{decode, encode, now, ClientMsg, LobbyEntry, ServerMsg, HASH_INTERVAL, PROTOCOL_ID};

#[derive(Clone, Debug)]
pub struct ServerOptions {
    pub bind: SocketAddr,
    /// Total parcels / players (2..=8). Unfilled seats go to the AI.
    pub slots: u8,
    pub seed: u64,
    pub config: Config,
    /// Name of the local player when the host also plays; `None` = dedicated.
    pub host_name: Option<String>,
}

struct Remote {
    name: String,
    player: Option<PlayerId>,
    /// Has introduced itself; only then is it listed or seated.
    hello: bool,
    /// Has received a snapshot and may be sent `Tick` messages.
    synced: bool,
}

pub struct HostServer {
    renet: RenetServer,
    transport: NetcodeServerTransport,
    opts: ServerOptions,
    clients: BTreeMap<ClientId, Remote>,
    pub session: Option<Session>,
    /// The host's own seat, if it plays.
    pub local_player: Option<PlayerId>,
    /// (tick, hash) of recent authoritative states.
    hashes: VecDeque<(u64, u64)>,
    /// Joiners waiting for the system command that seats them to be applied.
    pending_welcome: Vec<ClientId>,
    /// Human-readable log for UIs / stdout.
    pub log: Vec<String>,
}

impl HostServer {
    pub fn bind(opts: ServerOptions) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(opts.bind)?;
        let public = socket.local_addr()?;
        let config = ServerConfig {
            current_time: now(),
            max_clients: 8,
            protocol_id: PROTOCOL_ID,
            public_addresses: vec![public],
            authentication: ServerAuthentication::Unsecure,
        };
        let transport = NetcodeServerTransport::new(config, socket)?;
        let mut s = Self {
            renet: RenetServer::new(ConnectionConfig::default()),
            transport,
            opts,
            clients: BTreeMap::new(),
            session: None,
            local_player: None,
            hashes: VecDeque::new(),
            pending_welcome: Vec::new(),
            log: Vec::new(),
        };
        s.say(format!("Hosting on {public}"));
        Ok(s)
    }

    /// Resume a saved game instead of starting a new one from the lobby.
    pub fn load(&mut self, state: GameState) {
        self.session = Some(Session::new(state));
        if self.opts.host_name.is_some() {
            self.local_player = Some(PlayerId(0));
        }
        self.say("Loaded saved game; players can join and take over AI or empty parcels.".into());
    }

    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.transport.addresses().first().copied()
    }

    pub fn started(&self) -> bool {
        self.session.is_some()
    }

    pub fn state(&self) -> Option<&GameState> {
        self.session.as_ref().map(|s| &s.state)
    }

    fn say(&mut self, s: String) {
        println!("[host] {s}");
        self.log.push(s);
    }

    pub fn lobby(&self) -> Vec<LobbyEntry> {
        let mut v = Vec::new();
        if let Some(n) = &self.opts.host_name {
            v.push(LobbyEntry { name: n.clone(), is_host: true });
        }
        v.extend(self.clients.values().filter(|c| c.hello).map(|c| LobbyEntry { name: c.name.clone(), is_host: false }));
        v
    }

    pub fn slots(&self) -> u8 {
        self.opts.slots
    }

    fn send(&mut self, client: ClientId, msg: &ServerMsg) {
        self.renet.send_message(client, DefaultChannel::ReliableOrdered, encode(msg));
    }

    fn broadcast_lobby(&mut self) {
        let msg = ServerMsg::Lobby { entries: self.lobby(), slots: self.opts.slots };
        let ids: Vec<ClientId> = self.clients.keys().copied().collect();
        for id in ids {
            self.send(id, &msg);
        }
    }

    /// Pump the network: accept connections, read messages. Call every frame.
    pub fn poll(&mut self, dt: Duration) {
        self.renet.update(dt);
        if let Err(e) = self.transport.update(dt, &mut self.renet) {
            self.say(format!("transport error: {e}"));
        }
        while let Some(ev) = self.renet.get_event() {
            match ev {
                ServerEvent::ClientConnected { client_id } => {
                    self.clients.insert(client_id, Remote { name: format!("guest-{client_id}"), player: None, hello: false, synced: false });
                }
                ServerEvent::ClientDisconnected { client_id, reason } => self.on_disconnect(client_id, reason.to_string()),
            }
        }
        let ids: Vec<ClientId> = self.clients.keys().copied().collect();
        for id in ids {
            while let Some(bytes) = self.renet.receive_message(id, DefaultChannel::ReliableOrdered) {
                match decode::<ClientMsg>(&bytes) {
                    Some(msg) => self.on_message(id, msg),
                    None => self.say(format!("client {id} sent garbage")),
                }
            }
        }
    }

    /// Send queued packets. Call after `poll`/`tick` each frame.
    pub fn flush(&mut self) {
        self.transport.send_packets(&mut self.renet);
    }

    fn on_disconnect(&mut self, id: ClientId, reason: String) {
        let Some(c) = self.clients.remove(&id) else { return };
        self.pending_welcome.retain(|&w| w != id);
        self.say(format!("{} left ({reason})", c.name));
        match (&mut self.session, c.player) {
            (Some(s), Some(p)) => {
                // Their neighbourhood keeps running under the AI until someone drops in.
                s.submit(
                    PlayerId::SYSTEM,
                    CommandKind::SetController {
                        player: p,
                        controller: Controller::Ai(AiStrategy::Developer),
                        name: format!("{} (AI)", c.name),
                    },
                );
            }
            _ => self.broadcast_lobby(),
        }
    }

    fn on_message(&mut self, id: ClientId, msg: ClientMsg) {
        match msg {
            ClientMsg::Hello { name } => {
                let name: String = name.chars().filter(|c| !c.is_control()).take(24).collect();
                match self.clients.get_mut(&id) {
                    Some(c) if !c.hello => {
                        c.name = name.clone();
                        c.hello = true;
                    }
                    _ => return,
                }
                self.say(format!("{name} connected"));
                if self.session.is_some() {
                    self.seat_late_joiner(id);
                } else {
                    self.broadcast_lobby();
                }
            }
            ClientMsg::Submit(kind) => {
                let Some(player) = self.clients.get(&id).and_then(|c| c.player) else { return };
                // The only validation the network needs: clients can't forge system
                // commands or act as someone else. Ownership and money are checked
                // by the simulation itself, identically everywhere.
                if kind.is_system() {
                    self.say(format!("client {id} tried a system command"));
                    return;
                }
                if let Some(s) = &mut self.session {
                    s.submit(player, kind);
                }
            }
            ClientMsg::Hash { tick, hash } => {
                let ours = self.hashes.iter().find(|(t, _)| *t == tick).map(|(_, h)| *h);
                if let Some(ours) = ours {
                    if ours != hash {
                        let name = self.clients.get(&id).map(|c| c.name.clone()).unwrap_or_default();
                        self.say(format!("DESYNC with {name} at tick {tick}: {ours:016x} vs {hash:016x}; resyncing"));
                        self.send(id, &ServerMsg::Desync { tick });
                        self.send_welcome(id);
                    }
                }
            }
        }
    }

    /// Drop a joiner into an AI-run or empty parcel.
    fn seat_late_joiner(&mut self, id: ClientId) {
        let Some(s) = &mut self.session else { return };
        let taken: Vec<PlayerId> = self.clients.values().filter_map(|c| c.player).chain(self.local_player).collect();
        let seat = s
            .state
            .players
            .iter()
            .filter(|p| !taken.contains(&p.id))
            .find(|p| p.controller == Controller::Vacant)
            .or_else(|| s.state.players.iter().filter(|p| !taken.contains(&p.id)).find(|p| matches!(p.controller, Controller::Ai(_))))
            .map(|p| p.id);
        let Some(seat) = seat else {
            self.send(id, &ServerMsg::Notice("The game is full; you can't join.".into()));
            self.renet.disconnect(id);
            return;
        };
        let name = self.clients[&id].name.clone();
        s.submit(PlayerId::SYSTEM, CommandKind::SetController { player: seat, controller: Controller::Human, name: name.clone() });
        self.clients.get_mut(&id).unwrap().player = Some(seat);
        // Welcome after the seating command is applied, so their snapshot shows it.
        self.pending_welcome.push(id);
        self.say(format!("{name} takes over parcel {}", seat.0 + 1));
    }

    fn send_welcome(&mut self, id: ClientId) {
        let Some(s) = &self.session else { return };
        let Some(you) = self.clients.get(&id).and_then(|c| c.player) else { return };
        let snapshot = s.state.to_bytes();
        self.send(id, &ServerMsg::Welcome { you, snapshot });
        if let Some(c) = self.clients.get_mut(&id) {
            c.synced = true;
        }
    }

    /// Leave the lobby: seat the host and everyone connected, fill the rest with AI.
    pub fn start(&mut self) {
        if self.session.is_some() {
            return;
        }
        let mut players = Vec::new();
        if let Some(n) = &self.opts.host_name {
            players.push(PlayerSetup { name: n.clone(), controller: Controller::Human });
            self.local_player = Some(PlayerId(0));
        }
        // Clients still handshaking are seated as late joiners once they say hello.
        let ids: Vec<ClientId> = self.clients.iter().filter(|(_, c)| c.hello).map(|(id, _)| *id).collect();
        for &id in ids.iter().take(self.opts.slots as usize - players.len()) {
            let c = self.clients.get_mut(&id).unwrap();
            c.player = Some(PlayerId(players.len() as u8));
            players.push(PlayerSetup { name: c.name.clone(), controller: Controller::Human });
        }
        let mut ai = 0;
        while players.len() < self.opts.slots as usize {
            let strategy = if ai % 2 == 0 { AiStrategy::UtilityBaron } else { AiStrategy::Developer };
            players.push(PlayerSetup { name: ai_name(ai), controller: Controller::Ai(strategy) });
            ai += 1;
        }
        let state = GameState::new(&NewGame { seed: self.opts.seed, config: self.opts.config.clone(), players });
        self.session = Some(Session::new(state));
        for id in ids {
            if self.clients[&id].player.is_some() {
                self.send_welcome(id);
            } else {
                self.send(id, &ServerMsg::Notice("Game is full.".into()));
            }
        }
        self.say("Game started".into());
    }

    /// Queue a command from the host's own player.
    pub fn submit_local(&mut self, kind: CommandKind) {
        if let (Some(s), Some(me)) = (&mut self.session, self.local_player) {
            s.submit(me, kind);
        }
    }

    /// Advance the authoritative simulation one tick and broadcast what was applied.
    pub fn tick(&mut self) -> Option<TickReport> {
        let s = self.session.as_mut()?;
        s.ai_commands();
        let commands = s.take_pending();
        let tick = s.state.tick;
        let report = s.step_with(commands.clone());
        let after = s.state.tick;
        if after % HASH_INTERVAL == 0 {
            let h = s.state.hash();
            self.hashes.push_back((after, h));
            while self.hashes.len() > 64 {
                self.hashes.pop_front();
            }
        }
        let msg = ServerMsg::Tick { tick, commands };
        let synced: Vec<ClientId> = self.clients.iter().filter(|(_, c)| c.synced).map(|(id, _)| *id).collect();
        for id in synced {
            self.send(id, &msg);
        }
        for id in std::mem::take(&mut self.pending_welcome) {
            self.send_welcome(id);
        }
        Some(report)
    }

    /// Clients that have finished the handshake.
    pub fn connected(&self) -> usize {
        self.clients.values().filter(|c| c.hello).count()
    }

    pub fn disconnect_all(&mut self) {
        self.transport.disconnect_all(&mut self.renet);
    }
}
