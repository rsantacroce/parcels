use parcels_sim::systems::apply::plan_place;
use parcels_sim::*;

fn small_city(seed: u64) -> GameState {
    let mut config = Config::default();
    config.map_width = 32;
    config.map_height = 16;
    GameState::new(&NewGame::new(seed, config, "Testville"))
}

fn place(s: &mut Session, a: (u16, u16), b: (u16, u16), what: Buildable) -> TickReport {
    s.submit(CommandKind::Place { area: Area::Rect(Rect::from_corners(Pos::new(a.0, a.1), Pos::new(b.0, b.1))), what });
    s.tick()
}

/// Clear the river so tests aren't at the mercy of the seed.
fn dry(state: &mut GameState) {
    for t in &mut state.map.tiles {
        t.terrain = Terrain::Land;
    }
}

#[test]
fn cannot_build_off_the_map() {
    let mut s = Session::new(small_city(1));
    let r = place(&mut s, (28, 2), (40, 2), Buildable::Road(Road::Street));
    assert!(matches!(r.rejected[0].1, Rejection::OutsideMap(_)));
}

#[test]
fn placement_costs_money_and_blocks_when_broke() {
    let mut s = Session::new(small_city(1));
    dry(&mut s.state);
    let before = s.state.city.treasury;
    place(&mut s, (0, 0), (9, 0), Buildable::Road(Road::Street));
    let spent = before - s.state.city.treasury;
    // 10 roads plus one tick of upkeep.
    assert_eq!(spent, 10 * s.state.config.street_cost + 10 * s.state.config.street_upkeep);

    s.state.city.treasury = 5;
    let r = place(&mut s, (0, 2), (5, 2), Buildable::Road(Road::Street));
    assert!(matches!(r.rejected[0].1, Rejection::InsufficientFunds { .. }));
}

#[test]
fn power_reaches_connected_zones_until_it_runs_out() {
    let mut s = Session::new(small_city(3));
    dry(&mut s.state);
    place(&mut s, (15, 5), (15, 5), Buildable::Building(Building::WindTurbine));
    place(&mut s, (16, 5), (19, 5), Buildable::Zone(Zone::Residential, Density::Low));
    s.tick();
    for x in 16..=19 {
        assert!(s.state.map.tile(Pos::new(x, 5)).powered, "zone at x={x} powered by the turbine");
    }
    assert_eq!(s.state.city.stats.power.unserved, 0);

    // Far more housing than one turbine can carry: the tail end goes dark.
    let supply = s.state.city.stats.power.supply;
    let n = supply as u16 + 4;
    s.state.city.treasury = 100_000_00;
    place(&mut s, (20, 5), (19 + n.min(12), 5), Buildable::Zone(Zone::Residential, Density::Low));
    for p in Rect::from_corners(Pos::new(16, 5), Pos::new(31, 5)).iter() {
        s.state.map.tile_mut(p).level = 3;
    }
    s.tick();
    assert!(s.state.city.stats.power.unserved > 0);
    assert!(!s.state.map.tile(Pos::new(31, 5)).powered);
}

#[test]
fn a_served_neighbourhood_grows_and_earns() {
    let mut s = Session::new(small_city(5));
    dry(&mut s.state);
    place(&mut s, (0, 4), (14, 4), Buildable::Road(Road::Street));
    place(&mut s, (0, 4), (14, 4), Buildable::PowerLine);
    place(&mut s, (0, 4), (14, 4), Buildable::WaterPipe);
    // The 2x2 plant sits on rows 2-3, just above the road.
    place(&mut s, (0, 2), (0, 2), Buildable::Building(Building::CoalPlant));
    place(&mut s, (2, 3), (2, 3), Buildable::Building(Building::WaterPump));
    place(&mut s, (3, 3), (14, 3), Buildable::Zone(Zone::Residential, Density::Low));
    place(&mut s, (0, 5), (7, 5), Buildable::Zone(Zone::Commercial, Density::Low));
    place(&mut s, (8, 5), (14, 5), Buildable::Zone(Zone::Industrial, Density::Low));
    let start = s.state.city.treasury;
    for _ in 0..600 {
        s.tick();
    }
    let c = &s.state.city;
    assert!(c.stats.population > 100, "population {}", c.stats.population);
    assert!(c.stats.industrial_jobs + c.stats.commercial_jobs > 50);
    assert!(c.treasury > start, "treasury {} should beat {}", c.treasury, start);
}

#[test]
fn unpowered_zones_do_not_grow() {
    let mut s = Session::new(small_city(5));
    dry(&mut s.state);
    place(&mut s, (0, 4), (14, 4), Buildable::Road(Road::Street));
    place(&mut s, (2, 3), (14, 3), Buildable::Zone(Zone::Residential, Density::Low));
    for _ in 0..200 {
        s.tick();
    }
    assert_eq!(s.state.city.stats.population, 0);
}

#[test]
fn treasury_is_clamped_at_debt_floor() {
    let mut s = Session::new(small_city(5));
    dry(&mut s.state);
    place(&mut s, (0, 0), (15, 15), Buildable::Road(Road::Street));
    s.state.city.treasury = s.state.config.debt_floor + 1;
    for _ in 0..50 {
        s.tick();
    }
    assert_eq!(s.state.city.treasury, s.state.config.debt_floor);
}

#[test]
fn bulldoze_clears_everything() {
    let mut s = Session::new(small_city(5));
    dry(&mut s.state);
    place(&mut s, (0, 0), (3, 0), Buildable::Road(Road::Street));
    place(&mut s, (0, 0), (3, 0), Buildable::PowerLine);
    s.submit(CommandKind::Bulldoze { area: Area::Rect(Rect::from_corners(Pos::new(0, 0), Pos::new(3, 0))) });
    s.tick();
    assert!((0..4).all(|x| s.state.map.tile(Pos::new(x, 0)).is_bare()));
}

#[test]
fn preview_matches_application() {
    let mut s = Session::new(small_city(9));
    dry(&mut s.state);
    place(&mut s, (2, 2), (2, 2), Buildable::Building(Building::Park));
    let area = Area::Rect(Rect::from_corners(Pos::new(0, 2), Pos::new(5, 2)));
    let plan = plan_place(&s.state, &area, Buildable::Road(Road::Street)).unwrap();
    assert_eq!(plan.tiles.len(), 5, "park tile skipped");
    let before = s.state.city.treasury;
    s.submit(CommandKind::Place { area, what: Buildable::Road(Road::Street) });
    s.tick();
    let upkeep = s.state.city.stats.upkeep;
    assert_eq!(before - s.state.city.treasury, plan.cost + upkeep);
}

#[test]
fn big_buildings_occupy_and_clear_their_whole_footprint() {
    let mut s = Session::new(small_city(4));
    dry(&mut s.state);
    s.state.city.treasury = 100_000_00;
    let r = place(&mut s, (3, 3), (3, 3), Buildable::Building(Building::Stadium));
    assert!(r.rejected.is_empty(), "{:?}", r.rejected);
    let fp = Rect::square(Pos::new(3, 3), 3);
    for (i, p) in fp.iter().enumerate() {
        let t = s.state.map.tile(p);
        assert_eq!(t.kind, TileKind::Building(Building::Stadium));
        assert_eq!(t.part as usize, i);
    }
    // Overlapping placement is refused; footprints may not hang off the map.
    let r = place(&mut s, (4, 4), (4, 4), Buildable::Building(Building::School));
    assert_eq!(r.rejected[0].1, Rejection::NothingToDo);
    let r = place(&mut s, (31, 8), (31, 8), Buildable::Building(Building::School));
    assert!(matches!(r.rejected[0].1, Rejection::OutsideMap(_)));
    // Bulldozing any one tile removes the lot.
    s.submit(CommandKind::Bulldoze { area: Area::Tiles(vec![Pos::new(5, 5)]) });
    s.tick();
    assert!(fp.iter().all(|p| s.state.map.tile(p).is_bare()));
}

#[test]
fn forest_costs_extra_to_clear() {
    let mut s = Session::new(small_city(4));
    dry(&mut s.state);
    s.state.map.tile_mut(Pos::new(2, 2)).terrain = Terrain::Forest;
    let area = Area::Tiles(vec![Pos::new(2, 2), Pos::new(3, 2)]);
    let plan = plan_place(&s.state, &area, Buildable::Road(Road::Street)).unwrap();
    assert_eq!(plan.cost, 2 * s.state.config.street_cost + s.state.config.clear_forest_cost);
    s.submit(CommandKind::Place { area, what: Buildable::Road(Road::Street) });
    s.tick();
    assert_eq!(s.state.map.tile(Pos::new(2, 2)).terrain, Terrain::Land);
}

/// A powered service covers tiles around it, and only while powered.
#[test]
fn services_cover_when_powered() {
    let mut s = Session::new(small_city(6));
    dry(&mut s.state);
    s.state.config.fire_chance_per_million = 0;
    place(&mut s, (14, 5), (14, 5), Buildable::Building(Building::FireStation));
    s.tick();
    assert!(!s.state.map.tile(Pos::new(17, 5)).covered(Service::Fire), "unpowered station covers nothing");
    place(&mut s, (13, 5), (13, 5), Buildable::Building(Building::WindTurbine));
    s.tick();
    assert!(s.state.map.tile(Pos::new(14, 5)).powered);
    assert!(s.state.map.tile(Pos::new(17, 5)).covered(Service::Fire), "covers tiles nearby");
    assert!(!s.state.map.tile(Pos::new(31, 15)).covered(Service::Fire));
}

#[test]
fn housing_needs_a_school_to_pass_level_two() {
    let mut st = small_city(8);
    for t in &mut st.map.tiles {
        t.terrain = Terrain::Land;
    }
    let c = st.config.clone();
    let p = Pos::new(4, 4);
    let t = st.map.tile_mut(p);
    t.kind = TileKind::Zone(Zone::Residential, Density::Low);
    t.level = c.education_free_levels;
    t.powered = true;
    t.watered = true;
    assert_eq!(parcels_sim::systems::growth::growth_block(&c, t), Some("needs a school nearby"));
    t.coverage |= Service::Education.bit();
    assert_eq!(parcels_sim::systems::growth::growth_block(&c, t), None);
    t.level = c.low_density_max_level;
    assert!(parcels_sim::systems::growth::growth_block(&c, t).unwrap().starts_with("fully grown"));
}

#[test]
fn unprotected_buildings_burn_and_fire_stations_prevent_it() {
    let run = |protected: bool| {
        let mut s = Session::new(small_city(12));
        dry(&mut s.state);
        s.state.config.fire_chance_per_million = 200_000;
        for p in Rect::from_corners(Pos::new(0, 0), Pos::new(9, 9)).iter() {
            let t = s.state.map.tile_mut(p);
            t.kind = TileKind::Zone(Zone::Residential, Density::Low);
            t.level = 2;
            if protected {
                t.coverage = Service::Fire.bit();
            }
        }
        let r = s.tick();
        r.fires.len()
    };
    assert!(run(false) > 0);
    assert_eq!(run(true), 0);
}

#[test]
fn high_taxes_cost_approval_and_show_as_the_top_issue() {
    let run = |rate: u8| {
        let mut s = Session::new(small_city(5));
        dry(&mut s.state);
        s.submit(CommandKind::SetTaxRate { rate });
        for _ in 0..400 {
            s.tick();
        }
        s.state.city.stats
    };
    let fair = run(s_neutral());
    let harsh = run(20);
    assert!(harsh.approval < fair.approval, "{} vs {}", harsh.approval, fair.approval);
    assert_eq!(harsh.top_issue(), Some(Factor::Taxes));
    assert_eq!(fair.top_issue(), None);
}

fn s_neutral() -> u8 {
    Config::default().neutral_tax_rate
}
