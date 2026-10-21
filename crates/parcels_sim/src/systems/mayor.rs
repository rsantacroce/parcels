//! Stage 8: the mayor's office. Letters are checked every tick; once a month
//! goodwill fades, a resident may write in, and on election day the city votes.

use crate::catalog::Service;
use crate::city::{Factor, Utility};
use crate::map::Pos;
use crate::mayor::{is_home, near_park, resident_name, street_traffic, Ask, Election, Letter, LetterState};
use crate::rng::Rng;
use crate::state::{GameState, TickReport};

/// Answered letters kept for the inbox history.
const KEEP_ANSWERED: usize = 6;

pub fn run(state: &mut GameState, report: &mut TickReport) {
    let c = state.config.clone();
    let tpm = c.ticks_per_month.max(1) as u64;
    let term = c.election_interval_months.max(1) as u64 * tpm;
    if state.city.mayor.next_election == 0 {
        state.city.mayor.next_election = (state.tick / term + 1) * term;
    }

    // Letters: granted the moment the ask is met; missed at the deadline, or
    // when the writer's home is gone.
    let mut verdicts = Vec::new();
    for l in state.city.mayor.letters.iter().filter(|l| l.state == LetterState::Open) {
        if l.met(state) {
            verdicts.push((l.id, LetterState::Granted));
        } else if state.tick >= l.due || (l.ask.is_local() && !is_home(&state.map, l.at)) {
            verdicts.push((l.id, LetterState::Missed));
        }
    }
    let m = &mut state.city.mayor;
    for (id, verdict) in verdicts {
        if let Some(l) = m.letters.iter_mut().find(|l| l.id == id) {
            l.state = verdict;
        }
        if verdict == LetterState::Granted {
            m.goodwill += c.letter_reward;
            m.letters_granted += 1;
            report.letters_granted.push(id);
        } else {
            m.goodwill -= c.letter_penalty;
            m.letters_missed += 1;
            report.letters_missed.push(id);
        }
    }
    m.goodwill = m.goodwill.clamp(-c.goodwill_cap, c.goodwill_cap);

    // Monthly business happens on the month's last tick.
    if (state.tick + 1) % tpm != 0 {
        return;
    }
    let m = &mut state.city.mayor;
    let fade = m.goodwill * c.goodwill_fade_percent / 100;
    m.goodwill -= if fade == 0 { m.goodwill.signum() } else { fade };

    if state.tick + 1 >= m.next_election {
        let approval = state.city.stats.approval_percent();
        let won = approval >= c.reelection_percent;
        let e = Election { tick: state.tick + 1, approval, won };
        m.elections.push(e);
        m.next_election += term;
        report.election = Some(e);
    }

    let mut rng = Rng::for_tick(state.seed, state.tick, 0x4c45_5454);
    let open = state.city.mayor.open().count() as u32;
    if open < c.max_open_letters as u32 && state.city.stats.population >= c.letters_min_population && rng.chance(c.letter_chance_per_mille) {
        if let Some(letter) = compose(state, &mut rng, tpm) {
            report.letters_new.push(letter.id);
            let m = &mut state.city.mayor;
            m.next_letter += 1;
            m.letters.push(letter);
        }
    }

    // Forget the oldest answered letters.
    let m = &mut state.city.mayor;
    let answered = m.letters.iter().filter(|l| l.state != LetterState::Open).count();
    let mut drop = answered.saturating_sub(KEEP_ANSWERED);
    m.letters.retain(|l| {
        if drop > 0 && l.state != LetterState::Open {
            drop -= 1;
            return false;
        }
        true
    });
}

/// A new letter about what bothers residents, usually the top issue. `None`
/// when the chosen subject has nothing concrete to ask for, or a letter about
/// it is already open.
fn compose(state: &GameState, rng: &mut Rng, tpm: u64) -> Option<Letter> {
    let s = &state.city.stats;
    let map = &state.map;
    let factor = match s.top_issue() {
        Some(f) if rng.chance(700) => f,
        _ => Factor::ALL[rng.below(Factor::COUNT as u32) as usize],
    };
    let homes: Vec<Pos> = (0..map.len()).map(|i| map.pos(i)).filter(|&p| is_home(map, p)).collect();
    if homes.is_empty() {
        return None;
    }
    let any = |rng: &mut Rng, f: &dyn Fn(Pos) -> bool| {
        let v: Vec<Pos> = homes.iter().copied().filter(|&p| f(p)).collect();
        (!v.is_empty()).then(|| v[rng.below(v.len() as u32) as usize])
    };
    let worst = |key: &dyn Fn(Pos) -> u16, f: &dyn Fn(Pos) -> bool| homes.iter().copied().filter(|&p| f(p)).max_by_key(|&p| (key(p), std::cmp::Reverse(p)));
    let uncovered = |sv: Service| move |p: Pos| !map.tile(p).covered(sv);
    let neutral = state.config.neutral_tax_rate;

    let (ask, at) = match factor {
        Factor::Jobs if s.jobless_percent > 8 => (Ask::JoblessAtMost((s.jobless_percent as u32 * 2 / 3).max(5) as u8), any(rng, &|_| true)?),
        Factor::Utilities if s.power.unserved > 0 => (Ask::Supply(Utility::Power), any(rng, &|p| !map.tile(p).powered).or_else(|| any(rng, &|_| true))?),
        Factor::Utilities if s.water.unserved > 0 => (Ask::Supply(Utility::Water), any(rng, &|p| !map.tile(p).watered).or_else(|| any(rng, &|_| true))?),
        Factor::Safety => match worst(&|p| map.tile(p).crime, &|p| uncovered(Service::Police)(p) && map.tile(p).crime > 20) {
            Some(p) => (Ask::Service(Service::Police), p),
            None => (Ask::Service(Service::Fire), any(rng, &|p| uncovered(Service::Fire)(p) && map.tile(p).level >= 2)?),
        },
        Factor::Environment => {
            let p = worst(&|p| map.tile(p).pollution, &|p| map.tile(p).pollution > 30)?;
            (Ask::CleanAir(map.tile(p).pollution / 2), p)
        }
        Factor::Services => match any(rng, &uncovered(Service::Education)) {
            Some(p) => (Ask::Service(Service::Education), p),
            None => (Ask::Service(Service::Health), any(rng, &uncovered(Service::Health))?),
        },
        Factor::Taxes if state.city.tax_rate > neutral => (Ask::TaxesAtMost(neutral), any(rng, &|_| true)?),
        Factor::Traffic => {
            let p = worst(&|p| street_traffic(map, p), &|p| street_traffic(map, p) > 30)?;
            (Ask::QuietStreets(street_traffic(map, p) * 2 / 3), p)
        }
        Factor::Neighbourhoods => (Ask::Park, any(rng, &|p| !near_park(map, p))?),
        Factor::Budget if state.city.treasury < 0 => (Ask::OutOfDebt, any(rng, &|_| true)?),
        _ => return None,
    };
    if state.city.mayor.open().any(|l| l.ask.same_kind(ask)) {
        return None;
    }
    let id = state.city.mayor.next_letter;
    let letter = Letter {
        id,
        from: resident_name(rng.below(1000), rng.below(1000)),
        ask,
        at,
        sent: state.tick + 1,
        due: state.tick + 1 + state.config.letter_months as u64 * tpm,
        state: LetterState::Open,
    };
    (!letter.met(state)).then_some(letter)
}
