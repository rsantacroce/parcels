//! Real sockets on localhost: a host and two clients stay in lockstep.

use std::time::Duration;

use parcels_net::{HostServer, NetClient, ServerOptions};
use parcels_sim::{Area, Buildable, CommandKind, Config, Controller, Pos, Road};

fn pump(host: &mut HostServer, clients: &mut [&mut NetClient], frames: usize) {
    let dt = Duration::from_millis(5);
    for _ in 0..frames {
        host.poll(dt);
        host.flush();
        for c in clients.iter_mut() {
            c.poll(dt);
            while c.step().is_some() {}
            c.flush();
        }
        std::thread::sleep(dt);
    }
}

fn host(port: u16) -> HostServer {
    HostServer::bind(ServerOptions {
        bind: format!("127.0.0.1:{port}").parse().unwrap(),
        slots: 4,
        seed: 99,
        config: Config::default(),
        terrain: Default::default(),
        host_name: Some("Host".into()),
    })
    .unwrap()
}

#[test]
fn lockstep_over_udp() {
    let mut h = host(45811);
    let addr = h.local_addr().unwrap();
    let mut a = NetClient::connect(addr, "Alice").unwrap();
    let mut b = NetClient::connect(addr, "Bob").unwrap();
    for _ in 0..200 {
        pump(&mut h, &mut [&mut a, &mut b], 1);
        if h.lobby().len() == 3 && a.lobby.len() == 3 && b.lobby.len() == 3 {
            break;
        }
    }
    assert_eq!(h.lobby().len(), 3, "both clients in lobby");
    h.start();
    pump(&mut h, &mut [&mut a, &mut b], 20);
    let (ma, mb) = (a.me.expect("alice seated"), b.me.expect("bob seated"));
    assert_ne!(ma, mb);

    // Alice builds a road in her parcel; the host builds in its own.
    let parcel = a.state.as_ref().unwrap().parcels_of(ma).next().unwrap().clone();
    let p = Pos::new(parcel.rect.min.x + 2, parcel.rect.min.y + 2);
    a.submit(CommandKind::Place { parcel: parcel.id, area: Area::Tiles(vec![p]), what: Buildable::Road(Road::Street) });
    // Alice also tries to build in Bob's parcel: must be rejected everywhere.
    let bobs = a.state.as_ref().unwrap().parcels_of(mb).next().unwrap().clone();
    a.submit(CommandKind::Place { parcel: bobs.id, area: Area::Tiles(vec![bobs.rect.min]), what: Buildable::Road(Road::Street) });
    // And forge a system command: dropped by the host.
    a.submit(CommandKind::SetController { player: mb, controller: Controller::Vacant, name: "pwned".into() });

    for _ in 0..120 {
        pump(&mut h, &mut [&mut a, &mut b], 1);
        h.tick();
    }
    pump(&mut h, &mut [&mut a, &mut b], 40);

    let hs = h.state().unwrap();
    let (sa, sb) = (a.state.as_ref().unwrap(), b.state.as_ref().unwrap());
    assert_eq!(sa.tick, hs.tick, "alice caught up");
    assert_eq!(sb.tick, hs.tick, "bob caught up");
    assert_eq!(sa.hash(), hs.hash());
    assert_eq!(sb.hash(), hs.hash());
    assert_eq!(hs.map.tile(p).kind, parcels_sim::TileKind::Road(Road::Street));
    assert_eq!(hs.map.tile(bobs.rect.min).kind, parcels_sim::TileKind::Empty);
    assert_eq!(hs.players[mb.index()].name, "Bob");
    assert_eq!(a.desyncs + b.desyncs, 0);
}

#[test]
fn late_joiner_takes_over_ai_parcel() {
    let mut h = host(45812);
    let addr = h.local_addr().unwrap();
    h.start();
    for _ in 0..100 {
        h.poll(Duration::from_millis(1));
        h.tick();
        h.flush();
    }
    let mut c = NetClient::connect(addr, "Late").unwrap();
    for _ in 0..300 {
        pump(&mut h, &mut [&mut c], 1);
        h.tick();
        if c.state.as_ref().is_some_and(|s| s.tick + 1 >= h.state().unwrap().tick) && c.me.is_some() {
            break;
        }
    }
    pump(&mut h, &mut [&mut c], 30);
    let me = c.me.expect("seated");
    let hs = h.state().unwrap();
    assert_eq!(hs.players[me.index()].controller, Controller::Human);
    assert_eq!(hs.players[me.index()].name, "Late");
    assert_eq!(c.state.as_ref().unwrap().hash(), hs.hash());

    // When they leave, the AI takes the parcel back.
    c.disconnect();
    for _ in 0..100 {
        h.poll(Duration::from_millis(5));
        h.tick();
        h.flush();
        std::thread::sleep(Duration::from_millis(2));
        if matches!(h.state().unwrap().players[me.index()].controller, Controller::Ai(_)) {
            break;
        }
    }
    assert!(matches!(h.state().unwrap().players[me.index()].controller, Controller::Ai(_)));
}

#[test]
fn big_map_snapshot_reaches_a_late_joiner() {
    let mut config = Config::default();
    config.map_width = 256;
    config.map_height = 192;
    let mut h = HostServer::bind(ServerOptions {
        bind: "127.0.0.1:45813".parse().unwrap(),
        slots: 8,
        seed: 5,
        config,
        terrain: Default::default(),
        host_name: Some("Host".into()),
    })
    .unwrap();
    h.start();
    for _ in 0..60 {
        h.tick();
    }
    let size = h.state().unwrap().to_bytes().len();
    assert!(size > 500_000, "snapshot is {size} bytes; test should exercise a big one");
    let mut c = NetClient::connect(h.local_addr().unwrap(), "Late").unwrap();
    for _ in 0..1500 {
        pump(&mut h, &mut [&mut c], 1);
        // The welcome goes out on the tick that seats the joiner.
        if c.me.is_none() {
            h.tick();
        }
        if c.state.as_ref().is_some_and(|s| s.tick == h.state().unwrap().tick) {
            break;
        }
    }
    let cs = c.state.as_ref().expect("snapshot arrived");
    assert_eq!(cs.map.width, 256);
    assert_eq!(cs.hash(), h.state().unwrap().hash());
}
