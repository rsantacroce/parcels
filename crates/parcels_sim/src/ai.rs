//! Simple rule-based neighbours. The AI is just another command source: it reads
//! the state and returns commands, exactly like a human clicking. In multiplayer
//! only the host runs it, and its commands travel the same stream as everyone's.
//!
//! Deterministic: depends only on the state passed in.

use crate::catalog::{Building, Service};
use crate::command::{Area, CommandKind};
use crate::ids::PlayerId;
use crate::map::{Buildable, Density, Pos, Rect, Road, Terrain, TileKind, Zone};
use crate::player::{AiStrategy, Controller, Parcel, Utility};
use crate::state::GameState;
use crate::systems::apply::{plan_bulldoze, plan_place};

const RESERVE: i64 = 1_000_00;
const STREET: Buildable = Buildable::Road(Road::Street);

/// Should this player think on this tick? Players are staggered.
pub fn thinks_now(state: &GameState, player: PlayerId) -> bool {
    let interval = state.config.ai_think_interval.max(1) as u64;
    (state.tick + player.0 as u64) % interval == 0
}

/// Commands an AI player wants to issue this tick (usually zero or one).
pub fn plan(state: &GameState, player: PlayerId) -> Vec<CommandKind> {
    let Some(p) = state.player(player) else { return Vec::new() };
    let Controller::Ai(strategy) = p.controller else { return Vec::new() };
    if state.is_over() || !thinks_now(state, player) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for parcel in state.parcels_of(player) {
        let ctx = Ctx::new(state, player, strategy, parcel);
        if let Some(cmd) = ctx.next_build() {
            out.push(cmd);
            break;
        }
    }
    out.extend(policy(state, player, strategy));
    out
}

struct Ctx<'a> {
    state: &'a GameState,
    who: PlayerId,
    strategy: AiStrategy,
    parcel: &'a Parcel,
    /// Street rows, every third row: zones and buildings fill the two between.
    rows: Vec<u16>,
    spine: u16,
}

/// Columns kept free of zoning so plants, services and parks always have room:
/// a two-wide strip every `CIVIC_EVERY` columns, starting at the parcel edge.
const CIVIC_EVERY: u16 = 12;

impl<'a> Ctx<'a> {
    fn new(state: &'a GameState, who: PlayerId, strategy: AiStrategy, parcel: &'a Parcel) -> Self {
        let r = parcel.rect;
        Self { state, who, strategy, parcel, rows: (r.min.y + 1..r.max.y).step_by(3).collect(), spine: r.min.x + 2 }
    }

    fn civic(&self, p: Pos) -> bool {
        (p.x - self.parcel.rect.min.x) % CIVIC_EVERY < 2
    }

    /// Spare income per tick after upkeep and trade.
    fn margin(&self) -> i64 {
        let s = &self.state.players[self.who.index()].stats;
        s.taxes + s.trade - s.upkeep
    }

    /// Can we carry another `b` without sliding into debt?
    fn can_run(&self, b: Building) -> bool {
        self.margin() > self.state.config.building(b).upkeep * 2
    }

    fn treasury(&self) -> i64 {
        self.state.players[self.who.index()].treasury
    }

    fn try_place(&self, tiles: Vec<Pos>, what: Buildable) -> Option<CommandKind> {
        if tiles.is_empty() {
            return None;
        }
        let area = Area::Tiles(tiles);
        let plan = plan_place(self.state, self.who, self.parcel.id, &area, what).ok()?;
        (self.treasury() - plan.cost >= RESERVE).then_some(CommandKind::Place { parcel: self.parcel.id, area, what })
    }

    fn count(&self, b: Building) -> usize {
        let map = &self.state.map;
        self.parcel.rect.iter().filter(|&p| map.tile(p).kind == TileKind::Building(b) && map.tile(p).is_anchor()).count()
    }

    fn on_street_plan(&self, p: Pos) -> bool {
        p.x == self.spine || self.rows.contains(&p.y)
    }

    /// Place `b` on free land beside a road, nearest `target` if given (and, with
    /// `reach`, no further from it than that). Keeps clear of the planned
    /// streets so the grid can still be finished, and prefers the civic strips.
    fn place_building(&self, b: Building, target: Option<Pos>, reach: Option<u32>, want_river: bool) -> Option<CommandKind> {
        let map = &self.state.map;
        let size = b.size();
        let r = self.parcel.rect;
        if r.width() < size || r.height() < size {
            return None;
        }
        let mut spots: Vec<(u32, Pos)> = Vec::new();
        for y in r.min.y..=r.max.y + 1 - size {
            for x in r.min.x..=r.max.x + 1 - size {
                let fp = Rect::square(Pos::new(x, y), size);
                let free = fp.iter().all(|p| {
                    let t = map.tile(p);
                    t.kind == TileKind::Empty && t.terrain != Terrain::Water && !t.wire && !self.on_street_plan(p)
                });
                if !free {
                    continue;
                }
                let touches = |pred: &dyn Fn(usize) -> bool| fp.iter().any(|p| map.neighbors4(map.idx(p)).any(&pred));
                if !touches(&|j| map.tiles[j].kind.is_road()) {
                    continue;
                }
                let dist = target.map_or(0, |t| fp.center().manhattan(t));
                if reach.is_some_and(|r| dist > r) {
                    continue;
                }
                let wet = touches(&|j| map.tiles[j].terrain == Terrain::Water);
                let penalty = if want_river && !wet { 1000 } else { 0 } + if self.civic(fp.min) { 0 } else { 100 };
                spots.push((dist + penalty, fp.min));
            }
        }
        spots.sort();
        spots.into_iter().take(6).find_map(|(_, p)| self.try_place(vec![p], Buildable::Building(b)))
    }

    /// The first zone tile of `zone` (any if `None`) at `min_level`+ lacking `service`.
    fn uncovered(&self, service: Service, zone: Option<Zone>, min_level: u8) -> (usize, Option<Pos>) {
        let map = &self.state.map;
        let mut n = 0;
        let mut first = None;
        for p in self.parcel.rect.iter() {
            let t = map.tile(p);
            if t.kind.is_zone() && zone.is_none_or(|z| t.kind.zone() == Some(z)) && t.level >= min_level && !t.covered(service) {
                n += 1;
                first.get_or_insert(p);
            }
        }
        (n, first)
    }

    fn next_build(&self) -> Option<CommandKind> {
        let state = self.state;
        let r = self.parcel.rect;
        let map = &state.map;
        let stats = &state.players[self.who.index()].stats;
        let c = &state.config;
        let baron = self.strategy == AiStrategy::UtilityBaron;

        // 1. The spine and the first two streets. Both run edge to edge so they meet
        //    the neighbours' roads, joining everyone into one network.
        let spine: Vec<Pos> = (r.min.y..=r.max.y).map(|y| Pos::new(self.spine, y)).collect();
        let streets = |n: usize| -> Vec<Pos> {
            self.rows.iter().take(n).flat_map(|&y| (r.min.x..=r.max.x).map(move |x| Pos::new(x, y))).collect()
        };
        if let Some(c) = self.try_place(spine, STREET) {
            return Some(c);
        }
        if let Some(c) = self.try_place(streets(2), STREET) {
            return Some(c);
        }

        let roads: Vec<Pos> = r.iter().filter(|&p| map.tile(p).kind.is_road()).collect();
        let plants: usize = [Building::CoalPlant, Building::GasPlant, Building::WindTurbine, Building::SolarFarm, Building::NuclearPlant]
            .iter()
            .map(|&b| self.count(b))
            .sum();
        let pumps: usize = [Building::WaterPump, Building::WaterTower, Building::WaterTreatment].iter().map(|&b| self.count(b)).sum();

        // 2. Power. Barons overbuild big plants to sell; developers start with
        //    turbines, buy what neighbours offer, and add clean plants as they grow.
        let wants_power = if baron {
            plants == 0 || stats.power.supply < stats.power.demand + 100
        } else {
            plants == 0 || (stats.power.unserved > 0 && stats.power.bought < stats.power.unserved)
        };
        if wants_power {
            let choice = match (baron, plants) {
                (true, 0) => Building::CoalPlant,
                (true, _) if self.treasury() > 15_000_00 => Building::GasPlant,
                (true, _) => Building::CoalPlant,
                (false, _) if stats.power.demand < 60 || self.treasury() < 8_000_00 => Building::WindTurbine,
                (false, _) if self.treasury() > 12_000_00 => Building::SolarFarm,
                (false, _) => Building::CoalPlant,
            };
            for b in [choice, Building::CoalPlant, Building::WindTurbine] {
                if let Some(c) = self.place_building(b, None, None, false) {
                    return Some(c);
                }
            }
        }
        let unwired: Vec<Pos> = roads.iter().copied().filter(|&p| !map.tile(p).wire).collect();
        if let Some(c) = self.try_place(unwired, Buildable::PowerLine) {
            return Some(c);
        }

        // 3. Water: pipes along every road first, then another source only when
        //    every existing one is powered and supply really falls short.
        if pumps > 0 || stats.water.bought > 0 {
            let unpiped: Vec<Pos> = roads.iter().copied().filter(|&p| !map.tile(p).pipe).collect();
            if let Some(c) = self.try_place(unpiped, Buildable::WaterPipe) {
                return Some(c);
            }
        }
        let dark_pump = r.iter().any(|p| {
            let t = map.tile(p);
            t.is_anchor() && t.kind.building().is_some_and(|b| c.building(b).water_supply > 0) && !t.powered
        });
        let wants_water = !dark_pump
            && if baron {
                (pumps == 0 && stats.population > 0) || stats.water.supply < stats.water.demand + 60
            } else {
                stats.population > 60 && stats.water.supply + stats.water.bought < stats.water.demand
            };
        if wants_water {
            let b = if baron && self.treasury() > 20_000_00 && pumps > 0 { Building::WaterTreatment } else { Building::WaterPump };
            for b in [b, Building::WaterPump, Building::WaterTower] {
                if let Some(c) = self.place_building(b, None, None, true) {
                    return Some(c);
                }
            }
        }

        // 4. Services, where growth is held back or crime is biting. Only one new
        //    service at a time: wait until the last one is powered and covering.
        let dark_service = r.iter().any(|p| {
            let t = map.tile(p);
            t.is_anchor() && t.kind.building().is_some_and(|b| b.service().is_some()) && !t.powered
        });
        if stats.population >= 80 && !dark_service && stats.power.unserved == 0 {
            let crime: u32 = r.iter().map(|p| map.tile(p).crime as u32).max().unwrap_or(0);
            let wants = [
                (Building::School, self.uncovered(Service::Education, Some(Zone::Residential), c.education_free_levels), 6),
                (Building::FireStation, self.uncovered(Service::Fire, None, 2), 24),
                (Building::PoliceStation, if crime > 25 { self.uncovered(Service::Police, None, 2) } else { (0, None) }, 8),
                (
                    if self.treasury() > 20_000_00 { Building::Hospital } else { Building::Clinic },
                    self.uncovered(Service::Health, None, c.health_free_levels),
                    4,
                ),
            ];
            for (b, (count, at), threshold) in wants {
                if count >= threshold && self.can_run(b) {
                    // Only somewhere that actually covers the tile that asked for it.
                    let reach = c.building(b).service_radius as u32;
                    if let Some(c) = self.place_building(b, at, Some(reach), false) {
                        return Some(c);
                    }
                }
            }
        }

        // 5. Densify housing that has outgrown its low-density lots on good land.
        if stats.demand[Zone::Residential.index()] > 200 && self.treasury() > 8_000_00 {
            let tall: Vec<Pos> = r
                .iter()
                .filter(|&p| {
                    let t = map.tile(p);
                    t.kind == TileKind::Zone(Zone::Residential, Density::Low)
                        && t.level >= c.low_density_max_level
                        && t.land_value >= c.dense_min_land_value
                })
                .take(6)
                .collect();
            if let Some(c) = self.try_place(tall, Buildable::Zone(Zone::Residential, Density::High)) {
                return Some(c);
            }
        }

        // 6. Zone whatever is most in demand, if there isn't already empty zoning
        //    waiting and the lights are on for what's already there.
        let zoned = r.iter().any(|p| map.tile(p).kind.is_zone());
        let short = stats.power.unserved > stats.power.bought || (baron && stats.water.unserved > 40);
        if zoned && short {
            return None;
        }
        let schooled = r.iter().any(|p| map.tile(p).covered(Service::Education));
        let mut demand: Vec<(i32, Zone)> = Zone::ALL.iter().map(|&z| (stats.demand[z.index()], z)).collect();
        if !baron {
            demand[Zone::Residential.index()].0 += 150;
        }
        demand.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        for (d, zone) in demand {
            if d <= 0 {
                break;
            }
            if zone == Zone::Office && !schooled {
                continue;
            }
            let waiting = r.iter().filter(|&p| map.tile(p).kind.zone() == Some(zone) && map.tile(p).level == 0).count();
            if waiting >= 6 {
                continue;
            }
            let mut spots: Vec<Pos> = r
                .iter()
                .filter(|&p| p.x > self.spine && !self.civic(p))
                .filter(|&p| {
                    let t = map.tile(p);
                    t.kind == TileKind::Empty && t.terrain != Terrain::Water && !t.wire
                })
                .filter(|&p| map.neighbors4(map.idx(p)).any(|j| map.tiles[j].kind.is_road()))
                .collect();
            // Industry goes to the far end of the parcel, away from homes.
            if zone == Zone::Industrial {
                spots.reverse();
            }
            spots.truncate(8);
            let dense = !spots.is_empty()
                && zone != Zone::Industrial
                && self.treasury() > 10_000_00
                && spots.iter().map(|&p| map.tile(p).land_value as u32).sum::<u32>() / spots.len() as u32 >= c.dense_min_land_value as u32;
            let density = if dense { Density::High } else { Density::Low };
            if let Some(c) = self.try_place(spots, Buildable::Zone(zone, density)) {
                return Some(c);
            }
        }

        // 7. More streets only once there's nowhere left to zone.
        let free_frontage = r
            .iter()
            .filter(|&p| p.x > self.spine && !self.civic(p))
            .filter(|&p| map.tile(p).kind == TileKind::Empty && map.tile(p).terrain != Terrain::Water)
            .filter(|&p| map.neighbors4(map.idx(p)).any(|j| map.tiles[j].kind.is_road()))
            .count();
        for n in (3..=self.rows.len()).filter(|_| free_frontage < 4) {
            if let Some(c) = self.try_place(streets(n), STREET) {
                return Some(c);
            }
        }

        // 8. Bulldoze abandoned industry that is poisoning homes, then add amenities.
        let dead_ind: Vec<Pos> = r
            .iter()
            .filter(|&p| {
                let t = map.tile(p);
                t.kind.zone() == Some(Zone::Industrial) && t.level == 0 && t.pollution > 80
            })
            .take(4)
            .collect();
        if !dead_ind.is_empty() && self.treasury() > 10_000_00 {
            let area = Area::Tiles(dead_ind);
            if plan_bulldoze(state, self.who, self.parcel.id, &area).is_ok() {
                return Some(CommandKind::Bulldoze { parcel: self.parcel.id, area });
            }
        }
        if self.treasury() > 12_000_00 && self.margin() > 0 {
            let near_homes = r
                .iter()
                .filter(|&p| map.tile(p).kind.zone() == Some(Zone::Residential))
                .find(|&p| !map.within(p, 3).any(|(j, _)| map.tiles[j].kind.building().is_some_and(|b| b.service().is_none())));
            if let Some(home) = near_homes {
                let b = match (self.strategy, self.treasury() > 40_000_00) {
                    (AiStrategy::UtilityBaron, true) => Building::Monument,
                    (AiStrategy::Developer, true) => Building::SportsField,
                    (AiStrategy::Developer, false) => Building::Playground,
                    _ => Building::Park,
                };
                for b in [b, Building::Park].into_iter().filter(|&b| self.can_run(b)) {
                    if let Some(c) = self.place_building(b, Some(home), Some(6), false) {
                        return Some(c);
                    }
                }
            }
        }
        None
    }
}

fn policy(state: &GameState, who: PlayerId, strategy: AiStrategy) -> Vec<CommandKind> {
    let p = &state.players[who.index()];
    let s = &p.stats;
    let mut out = Vec::new();
    let (low, high) = match strategy {
        AiStrategy::UtilityBaron => (6, 11),
        AiStrategy::Developer => (4, 9),
    };
    let tax = if p.treasury < 3_000_00 {
        (p.tax_rate + 1).min(high)
    } else if p.treasury > 25_000_00 {
        p.tax_rate.saturating_sub(1).max(low)
    } else {
        p.tax_rate
    };
    if tax != p.tax_rate {
        out.push(CommandKind::SetTaxRate { rate: tax });
    }
    let base = state.config.default_utility_price;
    let markup = if strategy == AiStrategy::UtilityBaron { 2 } else { 0 };
    for (u, ledger, price) in [(Utility::Power, &s.power, p.power_price), (Utility::Water, &s.water, p.water_price)] {
        let has_surplus = ledger.supply > ledger.own_use;
        if has_surplus && price.is_none() {
            out.push(CommandKind::SetUtilityPrice { utility: u, price: Some(base + markup) });
        }
    }
    out
}
