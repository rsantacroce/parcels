//! Stage 6: the daily commute. Residents travel along the roads to the nearest
//! jobs, and road traffic is what that adds up to.
//!
//! An aggregate model, not agents: one cheapest-route search from every job
//! towards every road (so each road knows its way to work), then each home's
//! commuters join the network at their nearest road and flow down that tree.
//! Avenues are quicker, so routes favour them. Homes and jobs both near a
//! working bus stop send a share of their trips by bus, off the roads.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::catalog::{Building, Service};
use crate::map::{Road, TileKind, Zone};
use crate::region::{self, Side};
use crate::state::GameState;

const NONE: u32 = u32::MAX;

/// Connected road networks, and which ones a powered depot runs buses on.
pub struct BusNetwork {
    /// Network id per tile (roads only; `NONE` elsewhere).
    pub net: Vec<u32>,
    /// Riders each network's depots can carry (0 = no buses).
    pub capacity: Vec<u32>,
}

impl BusNetwork {
    pub fn new(state: &GameState) -> Self {
        let map = &state.map;
        let mut net = vec![NONE; map.len()];
        let mut count = 0u32;
        let mut stack = Vec::new();
        for i in 0..map.len() {
            if net[i] != NONE || !map.tiles[i].kind.is_road() {
                continue;
            }
            net[i] = count;
            stack.push(i);
            while let Some(j) = stack.pop() {
                for k in map.neighbors4(j) {
                    if net[k] == NONE && map.tiles[k].kind.is_road() {
                        net[k] = count;
                        stack.push(k);
                    }
                }
            }
            count += 1;
        }
        let mut capacity = vec![0u32; count as usize];
        let riders = state.config.bus_depot_riders;
        for i in 0..map.len() {
            let t = &map.tiles[i];
            let draws = state.config.building(Building::BusDepot).power_draw > 0;
            if t.kind != TileKind::Building(Building::BusDepot) || !t.is_anchor() || (draws && !t.powered) {
                continue;
            }
            let mut nets: Vec<u32> = Vec::new();
            for p in map.footprint_at(map.pos(i)).iter() {
                for k in map.neighbors4(map.idx(p)) {
                    if net[k] != NONE && !nets.contains(&net[k]) {
                        nets.push(net[k]);
                    }
                }
            }
            // A depot touching several networks serves the first it meets.
            if let Some(&n) = nets.first() {
                capacity[n as usize] += riders;
            }
        }
        Self { net, capacity }
    }

    /// The bus network a tile's neighbouring roads belong to, if buses run on it.
    fn served(&self, state: &GameState, idx: usize) -> Option<u32> {
        state.map.neighbors4(idx).map(|k| self.net[k]).find(|&n| n != NONE && self.capacity[n as usize] > 0)
    }

    pub fn stop_works(&self, state: &GameState, idx: usize) -> bool {
        self.served(state, idx).is_some()
    }
}

/// The two kinds of line stations run on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    Rail,
    Subway,
}

impl Line {
    fn track(self, t: &crate::map::Tile) -> bool {
        match self {
            Line::Rail => t.rail,
            Line::Subway => t.subway,
        }
    }

    fn station(self) -> Building {
        match self {
            Line::Rail => Building::TrainStation,
            Line::Subway => Building::SubwayStation,
        }
    }
}

/// Connected lines of one kind, how many working stations each has, and which
/// map edges it runs off (toward a neighbour; railways only).
pub struct RailNetwork {
    pub line: Line,
    pub net: Vec<u32>,
    pub stations: Vec<u32>,
    pub crosses: Vec<[bool; 4]>,
}

impl RailNetwork {
    pub fn new(state: &GameState) -> Self {
        Self::of(state, Line::Rail)
    }

    pub fn of(state: &GameState, line: Line) -> Self {
        let map = &state.map;
        let mut net = vec![NONE; map.len()];
        let mut count = 0u32;
        let mut stack = Vec::new();
        for i in 0..map.len() {
            if net[i] != NONE || !line.track(&map.tiles[i]) {
                continue;
            }
            net[i] = count;
            stack.push(i);
            while let Some(j) = stack.pop() {
                for k in map.neighbors4(j) {
                    if net[k] == NONE && line.track(&map.tiles[k]) {
                        net[k] = count;
                        stack.push(k);
                    }
                }
            }
            count += 1;
        }
        let mut crosses = vec![[false; 4]; count as usize];
        for side in Side::ALL {
            if state.neighbours[side.index()].is_none() || line == Line::Subway {
                continue;
            }
            for p in side.edge(map) {
                let n = net[map.idx(p)];
                if n != NONE {
                    crosses[n as usize][side.index()] = true;
                }
            }
        }
        let mut rails = Self { line, net, stations: vec![0; count as usize], crosses };
        for i in 0..map.len() {
            if let Some(n) = rails.line_of(state, i) {
                rails.stations[n as usize] += 1;
            }
        }
        rails
    }

    /// The line a powered station anchor at `idx` sits on (or beside).
    fn line_of(&self, state: &GameState, idx: usize) -> Option<u32> {
        let map = &state.map;
        let t = &map.tiles[idx];
        let b = self.line.station();
        let draws = state.config.building(b).power_draw > 0;
        if t.kind != TileKind::Building(b) || !t.is_anchor() || (draws && !t.powered) {
            return None;
        }
        let fp = map.footprint_at(map.pos(idx));
        fp.iter()
            .map(|p| map.idx(p))
            .chain(fp.iter().flat_map(|p| map.neighbors4(map.idx(p))))
            .map(|k| self.net[k])
            .find(|&n| n != NONE)
    }

    /// The line of a working station: one with somewhere to go, either
    /// another station or a neighbouring city down the track.
    pub fn station_net(&self, state: &GameState, idx: usize) -> Option<u32> {
        let n = self.line_of(state, idx)?;
        (self.stations[n as usize] >= 2 || self.crosses[n as usize].iter().any(|&c| c)).then_some(n)
    }
}

fn travel_cost(c: &crate::config::Config, kind: TileKind) -> u32 {
    match kind {
        TileKind::Road(Road::Avenue) => c.avenue_travel_cost.max(1),
        _ => c.street_travel_cost.max(1),
    }
}

/// Rounds of job search: commuters who find every nearby job taken look
/// again, farther out, up to this many times.
const ROUNDS: usize = 16;

pub fn run(state: &mut GameState) {
    // Commutes change slowly; refresh every few ticks.
    if state.tick % state.config.commute_interval.max(1) as u64 != 0 {
        return;
    }
    let c = &state.config;
    let map = &state.map;
    let n = map.len();
    let reach = c.road_access_radius;
    let share = |v: u32| v * c.commute_percent as u32 / 100;

    // Each job tile's workers arrive at its nearest road. Capacity is in trips.
    let mut open = vec![0u32; n];
    for i in 0..n {
        let t = &map.tiles[i];
        let jobs = match t.kind {
            TileKind::Zone(z, _) if z != Zone::Residential => state.occupants(i),
            TileKind::Building(b) if t.is_anchor() => {
                let s = c.building(b);
                if t.powered || s.power_draw == 0 { s.jobs as u32 } else { 0 }
            }
            _ => 0,
        };
        if jobs == 0 {
            continue;
        }
        let at = map.footprint_at(map.pos(i)).center();
        let door = map.within(at, reach + 1).filter(|&(j, _)| map.tiles[j].kind.is_road()).min_by_key(|&(j, d)| (d, j));
        if let Some((j, _)) = door {
            open[j] += share(jobs * c.workers_per_100_jobs.max(0) as u32 / 100);
        }
    }

    // Homes, with the trips they still have to place.
    let mut homes: Vec<(usize, u32)> = Vec::new();
    let mut stranded = 0u32;
    for i in 0..n {
        let t = &map.tiles[i];
        if t.kind.zone() == Some(Zone::Residential) && t.level > 0 {
            let people = state.occupants(i);
            if map.within(map.pos(i), reach).any(|(j, _)| map.tiles[j].kind.is_road()) {
                homes.push((i, share(people)));
            } else {
                stranded += people;
            }
        }
    }
    let mut reached = vec![false; homes.len()];

    // Jobs across each border, shared by trains and cars.
    let links = region::links(state);
    let mut pool = [0u32; 4];
    for side in Side::ALL {
        if let Some(nb) = &state.neighbours[side.index()] {
            pool[side.index()] = share(nb.open_jobs);
        }
    }

    // The subway and trains first: they skip the roads altogether.
    let (mut commuters, mut minutes, mut out) = (0u32, 0u64, 0u32);
    let tubes = RailNetwork::of(state, Line::Subway);
    let subway = by_train(state, &tubes, &mut open, &mut homes, &mut reached, &mut pool);
    let rails = RailNetwork::new(state);
    let trains = by_train(state, &rails, &mut open, &mut homes, &mut reached, &mut pool);
    commuters += subway.local + trains.local + trains.abroad;
    out += trains.abroad;
    minutes += SUBWAY_MINUTES * subway.local as u64 + TRAIN_MINUTES * (trains.local + trains.abroad) as u64;

    // What's left of a neighbour's jobs waits at the roads that cross into it.
    let edge_roads = |side: Side| -> Vec<usize> {
        side.edge(map).into_iter().map(|p| map.idx(p)).filter(|&j| map.tiles[j].kind.is_road()).collect()
    };
    let mut abroad = vec![0u32; n];
    for side in Side::ALL {
        if links[side.index()].road {
            for (j, v) in spread(&edge_roads(side), pool[side.index()]) {
                abroad[j] += v;
                open[j] += v;
            }
        }
    }

    let buses = BusNetwork::new(state);
    let mut left = buses.capacity.clone();
    let mut flow = vec![0u32; n];
    let mut riders = 0u32;
    for _ in 0..ROUNDS {
        if homes.iter().all(|h| h.1 == 0) {
            break;
        }
        let (dist, parent, root) = routes(state, &open);
        // Nearest homes claim jobs first.
        let mut queue: Vec<(u32, usize, usize)> = Vec::new();
        for (h, &(i, trips)) in homes.iter().enumerate() {
            if trips == 0 {
                continue;
            }
            let entry = map
                .within(map.pos(i), reach)
                .filter(|&(j, _)| dist[j] != NONE)
                .min_by_key(|&(j, d)| (dist[j] + d * c.street_travel_cost, j));
            if let Some((j, d)) = entry {
                queue.push((dist[j] + d * c.street_travel_cost, h, j));
                reached[h] = true;
            }
        }
        if queue.is_empty() {
            break;
        }
        let before = commuters;
        queue.sort_unstable();
        let mut round = vec![0u32; n];
        for (cost, h, entry) in queue {
            let dest = root[entry] as usize;
            let trips = homes[h].1.min(open[dest]);
            if trips == 0 {
                continue;
            }
            // Local jobs at this road fill before the ones across the border.
            let gone = trips.saturating_sub(open[dest] - abroad[dest]);
            abroad[dest] -= gone;
            out += gone;
            open[dest] -= trips;
            homes[h].1 -= trips;
            commuters += trips;
            minutes += (cost as u64 / c.street_travel_cost.max(1) as u64 + 2) * trips as u64;
            let mut cars = trips;
            let home = homes[h].0;
            if map.tiles[home].covered(Service::Transit) && map.tiles[dest].covered(Service::Transit) {
                let net = buses.net[entry] as usize;
                if net < left.len() {
                    let bus = (trips * c.bus_share_percent as u32 / 100).min(left[net]);
                    left[net] -= bus;
                    riders += bus;
                    cars -= bus;
                }
            }
            round[entry] += cars;
        }
        if commuters == before {
            break;
        }
        push(round, &dist, &parent, &mut flow);
    }

    // Job seekers from neighbouring cities drive in to the jobs still open.
    let mut local: Vec<u32> = (0..n).map(|i| open[i] - abroad[i]).collect();
    let mut inbound = 0u32;
    if local.iter().any(|&v| v > 0) {
        let (dist, parent, root) = routes(state, &local);
        let mut round = vec![0u32; n];
        for side in Side::ALL {
            let Some(nb) = &state.neighbours[side.index()] else { continue };
            if !links[side.index()].road {
                continue;
            }
            let gates: Vec<usize> = edge_roads(side).into_iter().filter(|&j| dist[j] != NONE).collect();
            for (j, v) in spread(&gates, share(nb.job_seekers)) {
                let dest = root[j] as usize;
                let trips = v.min(local[dest]);
                local[dest] -= trips;
                round[j] += trips;
                inbound += trips;
            }
        }
        push(round, &dist, &parent, &mut flow);
    }
    // Homes on roads that lead to no job at all.
    for (h, &(i, _)) in homes.iter().enumerate() {
        if !reached[h] {
            stranded += state.occupants(i);
        }
    }

    let tpm = c.ticks_per_month.max(1) as i64;
    let fares = (riders as i64 * c.bus_fare
        + (trains.local + trains.abroad) as i64 * c.train_fare
        + subway.local as i64 * c.subway_fare)
        / tpm;
    let (street, avenue, full) = (c.street_capacity.max(1), c.avenue_capacity.max(1), c.traffic_at_capacity);
    for i in 0..n {
        let t = &mut state.map.tiles[i];
        t.traffic = match t.kind {
            TileKind::Road(Road::Avenue) => flow[i] * full / avenue,
            TileKind::Road(Road::Street) => flow[i] * full / street,
            _ => 0,
        }
        .min(u16::MAX as u32) as u16;
    }
    let s = &mut state.city.stats;
    s.commuters = commuters;
    s.bus_riders = riders;
    s.stranded = stranded;
    s.commute_minutes = if commuters > 0 { (minutes / commuters as u64).min(u16::MAX as u64) as u16 } else { 0 };
    s.fares = fares;
    s.commuters_out = out;
    s.commuters_in = inbound;
    s.train_riders = trains.local + trains.abroad;
    s.subway_riders = subway.local;
}

/// Minutes a train or subway commute takes, door to door.
const TRAIN_MINUTES: u64 = 12;
const SUBWAY_MINUTES: u64 = 8;

struct TrainTrips {
    local: u32,
    abroad: u32,
}

/// Send a share of each home's trips by train, when the home is near a working
/// station: to open jobs near another station on the same line, or down the
/// line to a neighbouring city.
fn by_train(
    state: &GameState,
    rails: &RailNetwork,
    open: &mut [u32],
    homes: &mut [(usize, u32)],
    reached: &mut [bool],
    pool: &mut [u32; 4],
) -> TrainTrips {
    let (map, c) = (&state.map, &state.config);
    let lines = rails.stations.len();
    let mut trips = TrainTrips { local: 0, abroad: 0 };
    if lines == 0 {
        return trips;
    }
    // Which line serves each tile (the first working station that reaches it).
    let mut cover = vec![NONE; map.len()];
    let radius = c.building(rails.line.station()).service_radius;
    let (share, per_station) = match rails.line {
        Line::Rail => (c.train_share_percent, c.station_riders),
        Line::Subway => (c.subway_share_percent, c.subway_station_riders),
    };
    for i in 0..map.len() {
        if let Some(line) = rails.station_net(state, i) {
            let centre = map.footprint_at(map.pos(i)).center();
            for (j, _) in map.within(centre, radius) {
                if cover[j] == NONE {
                    cover[j] = line;
                }
            }
        }
    }
    let mut doors: Vec<Vec<usize>> = vec![Vec::new(); lines];
    for j in 0..map.len() {
        if open[j] > 0 && cover[j] != NONE {
            doors[cover[j] as usize].push(j);
        }
    }
    let mut next = vec![0usize; lines];
    let mut seats: Vec<u32> = rails.stations.iter().map(|&s| s * per_station).collect();
    for (h, home) in homes.iter_mut().enumerate() {
        let line = cover[home.0];
        if line == NONE || home.1 == 0 {
            continue;
        }
        let l = line as usize;
        let mut want = (home.1 * share as u32 / 100).min(seats[l]);
        let start = want;
        while want > 0 && next[l] < doors[l].len() {
            let j = doors[l][next[l]];
            let t = want.min(open[j]);
            open[j] -= t;
            want -= t;
            trips.local += t;
            if open[j] == 0 {
                next[l] += 1;
            }
        }
        for side in 0..4 {
            if rails.crosses[l][side] {
                let t = want.min(pool[side]);
                pool[side] -= t;
                want -= t;
                trips.abroad += t;
            }
        }
        let taken = start - want;
        seats[l] -= taken;
        home.1 -= taken;
        if taken > 0 {
            reached[h] = true;
        }
    }
    trips
}

/// Split `total` across `tiles`, the first taking the remainder.
fn spread(tiles: &[usize], total: u32) -> Vec<(usize, u32)> {
    if tiles.is_empty() {
        return Vec::new();
    }
    let each = total / tiles.len() as u32;
    let extra = total % tiles.len() as u32;
    tiles.iter().enumerate().map(|(k, &j)| (j, each + if k == 0 { extra } else { 0 })).collect()
}

/// Push trips that joined the roads on toward work along the route tree,
/// farthest roads first, adding them to `flow`.
fn push(mut round: Vec<u32>, dist: &[u32], parent: &[u32], flow: &mut [u32]) {
    let mut order: Vec<usize> = (0..round.len()).filter(|&i| dist[i] != NONE).collect();
    order.sort_unstable_by_key(|&i| Reverse((dist[i], i)));
    for &i in &order {
        if parent[i] != NONE {
            round[parent[i] as usize] += round[i];
        }
        flow[i] += round[i];
    }
}

/// Cheapest route from every road to a road with open jobs: distance, next
/// road along the way, and the job road it ends at. Ties break on tile index,
/// so the tree is the same on every machine.
fn routes(state: &GameState, open: &[u32]) -> (Vec<u32>, Vec<u32>, Vec<u32>) {
    let (map, c) = (&state.map, &state.config);
    let n = map.len();
    let mut dist = vec![NONE; n];
    let mut parent = vec![NONE; n];
    let mut root = vec![NONE; n];
    let mut heap = BinaryHeap::new();
    for i in 0..n {
        if open[i] > 0 {
            dist[i] = 0;
            root[i] = i as u32;
            heap.push(Reverse((0u32, i)));
        }
    }
    while let Some(Reverse((d, i))) = heap.pop() {
        if d > dist[i] {
            continue;
        }
        for j in map.neighbors4(i) {
            let tj = &map.tiles[j];
            if !tj.kind.is_road() {
                continue;
            }
            let nd = d + travel_cost(c, tj.kind);
            if nd < dist[j] {
                dist[j] = nd;
                parent[j] = i as u32;
                root[j] = root[i];
                heap.push(Reverse((nd, j)));
            }
        }
    }
    (dist, parent, root)
}
