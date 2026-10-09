use parcels_sim::systems::apply::plan_place;
use parcels_sim::*;

fn two_player(seed: u64) -> GameState {
    let mut config = Config::default();
    config.map_width = 32;
    config.map_height = 16;
    let players = vec![
        PlayerSetup { name: "A".into(), controller: Controller::Human },
        PlayerSetup { name: "B".into(), controller: Controller::Human },
    ];
    GameState::new(&NewGame { seed, config, players, terrain: TerrainSettings::default() })
}

fn place(s: &mut Session, who: u8, a: (u16, u16), b: (u16, u16), what: Buildable) -> TickReport {
    let parcel = s.state.parcels_of(PlayerId(who)).next().unwrap().id;
    s.submit(
        PlayerId(who),
        CommandKind::Place { parcel, area: Area::Rect(Rect::from_corners(Pos::new(a.0, a.1), Pos::new(b.0, b.1))), what },
    );
    s.tick()
}

/// Clear the river so tests aren't at the mercy of the seed.
fn dry(state: &mut GameState) {
    for t in &mut state.map.tiles {
        t.terrain = Terrain::Land;
    }
}

#[test]
fn parcels_partition_the_map() {
    let s = two_player(1);
    assert_eq!(s.parcels.len(), 2);
    for (i, t) in s.map.tiles.iter().enumerate() {
        let p = s.map.pos(i);
        let pc = s.parcel(t.parcel).expect("every tile owned");
        assert!(pc.rect.contains(p));
    }
}

#[test]
fn cannot_build_in_neighbours_parcel() {
    let mut s = Session::new(two_player(1));
    // Player 0 tries to build in parcel 1.
    s.submit(
        PlayerId(0),
        CommandKind::Place { parcel: ParcelId(1), area: Area::Tiles(vec![Pos::new(20, 2)]), what: Buildable::Road(Road::Street) },
    );
    let r = s.tick();
    assert_eq!(r.rejected.len(), 1);
    assert_eq!(r.rejected[0].1, Rejection::NotYourParcel);

    // Own parcel, but tiles straying over the border.
    let r = place(&mut s, 0, (10, 2), (20, 2), Buildable::Road(Road::Street));
    assert!(matches!(r.rejected[0].1, Rejection::OutsideParcel(_)));
}

#[test]
fn placement_costs_money_and_blocks_when_broke() {
    let mut s = Session::new(two_player(1));
    dry(&mut s.state);
    let before = s.state.players[0].treasury;
    place(&mut s, 0, (0, 0), (9, 0), Buildable::Road(Road::Street));
    let spent = before - s.state.players[0].treasury;
    // 10 roads plus one tick of upkeep.
    assert_eq!(spent, 10 * s.state.config.street_cost + 10 * s.state.config.street_upkeep);

    s.state.players[0].treasury = 5;
    let r = place(&mut s, 0, (0, 2), (5, 2), Buildable::Road(Road::Street));
    assert!(matches!(r.rejected[0].1, Rejection::InsufficientFunds { .. }));
}

#[test]
fn power_crosses_borders_and_is_sold() {
    let mut s = Session::new(two_player(3));
    dry(&mut s.state);
    // A: a turbine at the border. B: housing touching it.
    place(&mut s, 0, (15, 5), (15, 5), Buildable::Building(Building::WindTurbine));
    s.submit(PlayerId(0), CommandKind::SetUtilityPrice { utility: Utility::Power, price: Some(10) });
    place(&mut s, 1, (16, 5), (19, 5), Buildable::Zone(Zone::Residential, Density::Low));
    s.tick();
    for x in 16..=19 {
        assert!(s.state.map.tile(Pos::new(x, 5)).powered, "B's zone at x={x} powered by A's plant");
    }
    assert_eq!(s.state.players[1].stats.power.bought, 4);
    assert_eq!(s.state.players[0].stats.power.sold, 4);
    assert_eq!(s.state.players[0].stats.trade, 40);
    assert_eq!(s.state.players[1].stats.trade, -40);

    // Stop selling: B goes dark.
    s.submit(PlayerId(0), CommandKind::SetUtilityPrice { utility: Utility::Power, price: None });
    s.tick();
    assert!(!s.state.map.tile(Pos::new(16, 5)).powered);
    assert_eq!(s.state.players[1].stats.power.unserved, 4);
}

#[test]
fn a_served_neighbourhood_grows_and_earns() {
    let mut s = Session::new(two_player(5));
    dry(&mut s.state);
    place(&mut s, 0, (0, 4), (14, 4), Buildable::Road(Road::Street));
    place(&mut s, 0, (0, 4), (14, 4), Buildable::PowerLine);
    place(&mut s, 0, (0, 4), (14, 4), Buildable::WaterPipe);
    // The 2x2 plant sits on rows 2-3, just above the road.
    place(&mut s, 0, (0, 2), (0, 2), Buildable::Building(Building::CoalPlant));
    place(&mut s, 0, (2, 3), (2, 3), Buildable::Building(Building::WaterPump));
    place(&mut s, 0, (3, 3), (14, 3), Buildable::Zone(Zone::Residential, Density::Low));
    place(&mut s, 0, (0, 5), (7, 5), Buildable::Zone(Zone::Commercial, Density::Low));
    place(&mut s, 0, (8, 5), (14, 5), Buildable::Zone(Zone::Industrial, Density::Low));
    let start = s.state.players[0].treasury;
    for _ in 0..600 {
        s.tick();
    }
    let p = &s.state.players[0];
    assert!(p.stats.population > 100, "population {}", p.stats.population);
    assert!(p.stats.industrial_jobs + p.stats.commercial_jobs > 50);
    assert!(p.treasury > start, "treasury {} should beat {}", p.treasury, start);
}

#[test]
fn unpowered_zones_do_not_grow() {
    let mut s = Session::new(two_player(5));
    dry(&mut s.state);
    place(&mut s, 0, (0, 4), (14, 4), Buildable::Road(Road::Street));
    place(&mut s, 0, (2, 3), (14, 3), Buildable::Zone(Zone::Residential, Density::Low));
    for _ in 0..200 {
        s.tick();
    }
    assert_eq!(s.state.players[0].stats.population, 0);
}

#[test]
fn treasury_is_clamped_at_debt_floor() {
    let mut s = Session::new(two_player(5));
    dry(&mut s.state);
    place(&mut s, 0, (0, 0), (15, 15), Buildable::Road(Road::Street));
    s.state.players[0].treasury = s.state.config.debt_floor + 1;
    for _ in 0..50 {
        s.tick();
    }
    assert_eq!(s.state.players[0].treasury, s.state.config.debt_floor);
}

#[test]
fn bulldoze_clears_everything() {
    let mut s = Session::new(two_player(5));
    dry(&mut s.state);
    place(&mut s, 0, (0, 0), (3, 0), Buildable::Road(Road::Street));
    place(&mut s, 0, (0, 0), (3, 0), Buildable::PowerLine);
    s.submit(
        PlayerId(0),
        CommandKind::Bulldoze { parcel: ParcelId(0), area: Area::Rect(Rect::from_corners(Pos::new(0, 0), Pos::new(3, 0))) },
    );
    s.tick();
    assert!((0..4).all(|x| s.state.map.tile(Pos::new(x, 0)).is_bare()));
}

#[test]
fn preview_matches_application() {
    let mut s = Session::new(two_player(9));
    dry(&mut s.state);
    place(&mut s, 0, (2, 2), (2, 2), Buildable::Building(Building::Park));
    let area = Area::Rect(Rect::from_corners(Pos::new(0, 2), Pos::new(5, 2)));
    let plan = plan_place(&s.state, PlayerId(0), ParcelId(0), &area, Buildable::Road(Road::Street)).unwrap();
    assert_eq!(plan.tiles.len(), 5, "park tile skipped");
    let before = s.state.players[0].treasury;
    s.submit(PlayerId(0), CommandKind::Place { parcel: ParcelId(0), area, what: Buildable::Road(Road::Street) });
    s.tick();
    let upkeep = s.state.players[0].stats.upkeep;
    assert_eq!(before - s.state.players[0].treasury, plan.cost + upkeep);
}

#[test]
fn system_commands_need_system_author() {
    let mut s = Session::new(two_player(1));
    s.submit(PlayerId(0), CommandKind::SetController { player: PlayerId(1), controller: Controller::Vacant, name: "x".into() });
    let r = s.tick();
    assert_eq!(r.rejected[0].1, Rejection::NotSystem);
    s.submit(PlayerId::SYSTEM, CommandKind::SetController { player: PlayerId(1), controller: Controller::Ai(AiStrategy::Developer), name: "Bot".into() });
    let r = s.tick();
    assert!(r.rejected.is_empty());
    assert_eq!(s.state.players[1].name, "Bot");
}

#[test]
fn game_ends_at_time_limit_with_ranking() {
    let mut st = two_player(1);
    st.config.game_length_ticks = 10;
    let mut s = Session::new(st);
    s.state.players[1].treasury += 1_000_000;
    for _ in 0..10 {
        s.tick();
    }
    let out = s.state.outcome.as_ref().expect("game over");
    assert_eq!(out.ranking[0].0, PlayerId(1));
    let r = s.tick();
    assert_eq!(s.state.tick, 10, "no ticks after the end");
    assert!(r.applied.is_empty());
}

#[test]
fn big_buildings_occupy_and_clear_their_whole_footprint() {
    let mut s = Session::new(two_player(4));
    dry(&mut s.state);
    s.state.players[0].treasury = 100_000_00;
    let r = place(&mut s, 0, (3, 3), (3, 3), Buildable::Building(Building::Stadium));
    assert!(r.rejected.is_empty(), "{:?}", r.rejected);
    let fp = Rect::square(Pos::new(3, 3), 3);
    for (i, p) in fp.iter().enumerate() {
        let t = s.state.map.tile(p);
        assert_eq!(t.kind, TileKind::Building(Building::Stadium));
        assert_eq!(t.part as usize, i);
    }
    // Overlapping placement is refused; footprints may not cross the fence.
    let r = place(&mut s, 0, (4, 4), (4, 4), Buildable::Building(Building::School));
    assert_eq!(r.rejected[0].1, Rejection::NothingToDo);
    let r = place(&mut s, 0, (15, 8), (15, 8), Buildable::Building(Building::School));
    assert!(matches!(r.rejected[0].1, Rejection::OutsideParcel(_)));
    // Bulldozing any one tile removes the lot.
    s.submit(PlayerId(0), CommandKind::Bulldoze { parcel: ParcelId(0), area: Area::Tiles(vec![Pos::new(5, 5)]) });
    s.tick();
    assert!(fp.iter().all(|p| s.state.map.tile(p).is_bare()));
}

#[test]
fn forest_costs_extra_to_clear() {
    let mut s = Session::new(two_player(4));
    dry(&mut s.state);
    s.state.map.tile_mut(Pos::new(2, 2)).terrain = Terrain::Forest;
    let area = Area::Tiles(vec![Pos::new(2, 2), Pos::new(3, 2)]);
    let plan = plan_place(&s.state, PlayerId(0), ParcelId(0), &area, Buildable::Road(Road::Street)).unwrap();
    assert_eq!(plan.cost, 2 * s.state.config.street_cost + s.state.config.clear_forest_cost);
    s.submit(PlayerId(0), CommandKind::Place { parcel: ParcelId(0), area, what: Buildable::Road(Road::Street) });
    s.tick();
    assert_eq!(s.state.map.tile(Pos::new(2, 2)).terrain, Terrain::Land);
}

/// A powered service covers tiles around it, across the border, and only while powered.
#[test]
fn services_cover_across_borders_when_powered() {
    let mut s = Session::new(two_player(6));
    dry(&mut s.state);
    s.state.config.fire_chance_per_million = 0;
    place(&mut s, 0, (14, 5), (14, 5), Buildable::Building(Building::FireStation));
    s.tick();
    assert!(!s.state.map.tile(Pos::new(17, 5)).covered(Service::Fire), "unpowered station covers nothing");
    place(&mut s, 0, (13, 5), (13, 5), Buildable::Building(Building::WindTurbine));
    s.tick();
    assert!(s.state.map.tile(Pos::new(14, 5)).powered);
    assert!(s.state.map.tile(Pos::new(17, 5)).covered(Service::Fire), "covers the neighbour's side");
    assert!(!s.state.map.tile(Pos::new(31, 15)).covered(Service::Fire));
}

#[test]
fn housing_needs_a_school_to_pass_level_two() {
    let mut st = two_player(8);
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
        let mut s = Session::new(two_player(12));
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
