//! Hand-drawn vector icons and the cream "paper" palette. Icons are painted
//! with egui shapes, so they stay crisp at any interface size and need no
//! image assets.

use std::f32::consts::{PI, TAU};

use bevy_egui::egui::{self, Color32, Painter, Pos2, Rect, Shape, Stroke};
use parcels_sim::{Ask, Buildable, Building, Category, Factor, Road, Service, Utility, Zone};

/// The paper palette shared by every screen.
pub mod pal {
    use super::Color32;
    pub const INK: Color32 = Color32::from_rgb(58, 46, 36);
    pub const INK_SOFT: Color32 = Color32::from_rgb(118, 100, 82);
    pub const PAPER: Color32 = Color32::from_rgb(252, 246, 232);
    pub const PAPER_DIM: Color32 = Color32::from_rgb(244, 233, 209);
    pub const PAPER_DEEP: Color32 = Color32::from_rgb(232, 216, 184);
    pub const BORDER: Color32 = Color32::from_rgb(226, 164, 74);
    pub const ACCENT: Color32 = Color32::from_rgb(243, 183, 72);
    pub const ACCENT_SOFT: Color32 = Color32::from_rgb(250, 222, 160);
    pub const GOOD: Color32 = Color32::from_rgb(104, 150, 74);
    pub const BAD: Color32 = Color32::from_rgb(196, 78, 58);
    pub const WARN: Color32 = Color32::from_rgb(214, 140, 40);
    pub const NIGHT: Color32 = Color32::from_rgb(38, 42, 54);
    pub const SKY: Color32 = Color32::from_rgb(86, 140, 205);
    pub const LEAF: Color32 = Color32::from_rgb(92, 158, 76);
    pub const BARK: Color32 = Color32::from_rgb(140, 96, 62);
    pub const STEEL: Color32 = Color32::from_rgb(96, 104, 120);
    pub const GOLD: Color32 = Color32::from_rgb(236, 180, 50);
    pub const BRICK: Color32 = Color32::from_rgb(200, 92, 70);
    pub const NAVY: Color32 = Color32::from_rgb(60, 84, 140);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Inspect,
    Zones,
    Road,
    Avenue,
    Power,
    PowerLine,
    Water,
    Pipe,
    Services,
    Parks,
    Landmark,
    Bulldoze,
    DataViews,
    Budget,
    City,
    Mood,
    Gear,
    Walk,
    Pause,
    Play,
    Fast,
    Faster,
    People,
    Money,
    Calendar,
    Jobs,
    Sound,
    Muted,
    Save,
    Folder,
    Sparkle,
    Quit,
    Help,
    Leaf,
    Car,
    Health,
    School,
    Police,
    Fire,
    Shop,
    Factory,
    Office,
    House,
    Coins,
    Warning,
    Replay,
    Plant,
    Wind,
    Sun,
    Atom,
    Tower,
    Pump,
    Plane,
    Cube,
    Mail,
    Lock,
    Era,
    Bus,
    BusStop,
    Ballot,
    Mayor,
    Check,
    Cross,
}

impl Icon {
    pub fn for_category(c: Category) -> Icon {
        match c {
            Category::Transport => Icon::Road,
            Category::Zones => Icon::Zones,
            Category::Power => Icon::Power,
            Category::Water => Icon::Water,
            Category::Services => Icon::Services,
            Category::Parks => Icon::Parks,
            Category::Landmarks => Icon::Landmark,
        }
    }

    pub fn for_buildable(b: Buildable) -> Icon {
        match b {
            Buildable::Road(Road::Street) => Icon::Road,
            Buildable::Road(Road::Avenue) => Icon::Avenue,
            Buildable::PowerLine => Icon::PowerLine,
            Buildable::WaterPipe => Icon::Pipe,
            Buildable::Zone(z, _) => Icon::for_zone(z),
            Buildable::Building(b) => match b {
                Building::BusStop => Icon::BusStop,
                Building::BusDepot => Icon::Bus,
                Building::CoalPlant | Building::GasPlant => Icon::Plant,
                Building::WindTurbine => Icon::Wind,
                Building::SolarFarm => Icon::Sun,
                Building::NuclearPlant => Icon::Atom,
                Building::WaterPump | Building::WaterTreatment => Icon::Pump,
                Building::WaterTower => Icon::Tower,
                Building::PoliceStation => Icon::Police,
                Building::FireStation => Icon::Fire,
                Building::Clinic | Building::Hospital => Icon::Health,
                Building::School | Building::University => Icon::School,
                Building::Airport => Icon::Plane,
                Building::Monument | Building::TownHall => Icon::Landmark,
                _ => Icon::Parks,
            },
        }
    }

    pub fn for_zone(z: Zone) -> Icon {
        match z {
            Zone::Residential => Icon::House,
            Zone::Commercial => Icon::Shop,
            Zone::Industrial => Icon::Factory,
            Zone::Office => Icon::Office,
        }
    }

    pub fn for_ask(a: Ask) -> Icon {
        match a {
            Ask::Service(Service::Police) => Icon::Police,
            Ask::Service(Service::Fire) => Icon::Fire,
            Ask::Service(Service::Health) => Icon::Health,
            Ask::Service(Service::Education) => Icon::School,
            Ask::Service(Service::Transit) => Icon::BusStop,
            Ask::Park => Icon::Parks,
            Ask::TaxesAtMost(_) => Icon::Coins,
            Ask::Supply(Utility::Power) => Icon::Power,
            Ask::Supply(Utility::Water) => Icon::Water,
            Ask::JoblessAtMost(_) => Icon::Jobs,
            Ask::CleanAir(_) => Icon::Leaf,
            Ask::QuietStreets(_) => Icon::Car,
            Ask::OutOfDebt => Icon::Money,
        }
    }

    pub fn for_factor(f: Factor) -> Icon {
        match f {
            Factor::Jobs => Icon::Jobs,
            Factor::Utilities => Icon::Power,
            Factor::Safety => Icon::Police,
            Factor::Environment => Icon::Leaf,
            Factor::Services => Icon::School,
            Factor::Taxes => Icon::Coins,
            Factor::Traffic => Icon::Car,
            Factor::Neighbourhoods => Icon::Parks,
            Factor::Budget => Icon::Money,
        }
    }
}

/// Maps unit coordinates (0..1 inside `rect`) to the screen.
struct Pen<'a> {
    p: &'a Painter,
    r: Rect,
}

impl Pen<'_> {
    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.r.min + egui::vec2(x * self.r.width(), y * self.r.height())
    }
    fn s(&self, v: f32) -> f32 {
        v * self.r.width()
    }
    fn stroke(&self, w: f32, c: Color32) -> Stroke {
        Stroke::new(self.s(w).max(1.0), c)
    }
    fn circle(&self, x: f32, y: f32, r: f32, c: Color32) {
        self.p.circle_filled(self.at(x, y), self.s(r), c);
    }
    fn ring(&self, x: f32, y: f32, r: f32, w: f32, c: Color32) {
        self.p.circle_stroke(self.at(x, y), self.s(r), self.stroke(w, c));
    }
    fn line(&self, a: (f32, f32), b: (f32, f32), w: f32, c: Color32) {
        self.p.line_segment([self.at(a.0, a.1), self.at(b.0, b.1)], self.stroke(w, c));
    }
    fn path(&self, pts: &[(f32, f32)], w: f32, c: Color32) {
        self.p.add(Shape::line(pts.iter().map(|&(x, y)| self.at(x, y)).collect(), self.stroke(w, c)));
    }
    fn poly(&self, pts: &[(f32, f32)], c: Color32) {
        self.p.add(Shape::convex_polygon(pts.iter().map(|&(x, y)| self.at(x, y)).collect(), c, Stroke::NONE));
    }
    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, round: f32, c: Color32) {
        self.p.rect_filled(Rect::from_min_max(self.at(x0, y0), self.at(x1, y1)), self.s(round), c);
    }
    fn frame(&self, x0: f32, y0: f32, x1: f32, y1: f32, round: f32, w: f32, c: Color32) {
        self.p.rect_stroke(Rect::from_min_max(self.at(x0, y0), self.at(x1, y1)), self.s(round), self.stroke(w, c), egui::StrokeKind::Middle);
    }
    fn text(&self, x: f32, y: f32, size: f32, t: &str, c: Color32) {
        self.p.text(self.at(x, y), egui::Align2::CENTER_CENTER, t, egui::FontId::proportional(self.s(size)), c);
    }
    /// A filled star (or sparkle with few points) made of triangles.
    fn star(&self, cx: f32, cy: f32, outer: f32, inner: f32, points: usize, c: Color32) {
        let n = points * 2;
        let pt = |i: usize| {
            let a = -PI / 2.0 + i as f32 * TAU / n as f32;
            let r = if i % 2 == 0 { outer } else { inner };
            (cx + r * a.cos(), cy + r * a.sin())
        };
        for i in 0..n {
            self.poly(&[(cx, cy), pt(i), pt((i + 1) % n)], c);
        }
    }
    fn arc(&self, cx: f32, cy: f32, r: f32, from: f32, to: f32, w: f32, c: Color32) {
        let pts: Vec<(f32, f32)> =
            (0..=12).map(|i| from + (to - from) * i as f32 / 12.0).map(|a| (cx + r * a.cos(), cy + r * a.sin())).collect();
        self.path(&pts, w, c);
    }
}

/// Paint `icon` inside `rect`. `bg` is the colour behind it, used for cut-outs.
pub fn paint(p: &Painter, rect: Rect, icon: Icon, bg: Color32) {
    use pal::*;
    let k = Pen { p, r: rect };
    match icon {
        Icon::Inspect => {
            k.circle(0.42, 0.42, 0.26, SKY.gamma_multiply(0.35));
            k.ring(0.42, 0.42, 0.26, 0.09, INK);
            k.line((0.61, 0.61), (0.86, 0.86), 0.14, INK);
        }
        Icon::Zones => {
            let tints = [
                Color32::from_rgb(120, 190, 100),
                Color32::from_rgb(100, 150, 225),
                Color32::from_rgb(232, 186, 76),
                Color32::from_rgb(160, 130, 225),
            ];
            for (i, c) in tints.into_iter().enumerate() {
                let (x, y) = (0.12 + (i % 2) as f32 * 0.4, 0.12 + (i / 2) as f32 * 0.4);
                k.rect(x, y, x + 0.36, y + 0.36, 0.08, c);
            }
            k.frame(0.12, 0.12, 0.88, 0.88, 0.1, 0.06, INK);
        }
        Icon::Road | Icon::Avenue => {
            k.poly(&[(0.36, 0.08), (0.64, 0.08), (0.9, 0.92), (0.1, 0.92)], STEEL);
            if icon == Icon::Avenue {
                k.line((0.5, 0.12), (0.5, 0.9), 0.08, LEAF);
            } else {
                for i in 0..3 {
                    let y = 0.14 + i as f32 * 0.28;
                    k.line((0.5, y), (0.5, y + 0.14), 0.07, PAPER);
                }
            }
        }
        Icon::Power => {
            k.poly(&[(0.6, 0.06), (0.24, 0.56), (0.54, 0.56)], GOLD);
            k.poly(&[(0.46, 0.44), (0.76, 0.44), (0.38, 0.94)], GOLD);
            k.path(&[(0.6, 0.06), (0.24, 0.56), (0.5, 0.56), (0.38, 0.94), (0.76, 0.44), (0.5, 0.44), (0.6, 0.06)], 0.05, INK);
        }
        Icon::PowerLine => {
            k.path(&[(0.22, 0.92), (0.5, 0.1), (0.78, 0.92)], 0.07, INK);
            k.line((0.2, 0.3), (0.8, 0.3), 0.07, INK);
            k.line((0.28, 0.52), (0.72, 0.52), 0.07, INK);
            k.line((0.33, 0.7), (0.67, 0.7), 0.05, INK_SOFT);
            k.circle(0.2, 0.3, 0.06, GOLD);
            k.circle(0.8, 0.3, 0.06, GOLD);
        }
        Icon::Water | Icon::Pump => {
            k.poly(&[(0.5, 0.08), (0.27, 0.56), (0.73, 0.56)], SKY);
            k.circle(0.5, 0.62, 0.25, SKY);
            k.arc(0.5, 0.62, 0.14, 0.3, 1.4, 0.06, PAPER);
            if icon == Icon::Pump {
                k.rect(0.1, 0.84, 0.9, 0.94, 0.03, INK);
            }
        }
        Icon::Pipe => {
            k.line((0.08, 0.5), (0.92, 0.5), 0.22, STEEL);
            k.rect(0.22, 0.32, 0.32, 0.68, 0.02, INK);
            k.rect(0.68, 0.32, 0.78, 0.68, 0.02, INK);
            k.circle(0.5, 0.82, 0.07, SKY);
        }
        Icon::Services | Icon::Police => {
            let shield = [(0.5, 0.08), (0.84, 0.22), (0.8, 0.55), (0.5, 0.92), (0.2, 0.55), (0.16, 0.22)];
            k.poly(&shield, NAVY);
            k.path(&[shield[0], shield[1], shield[2], shield[3], shield[4], shield[5], shield[0]], 0.05, INK);
            k.star(0.5, 0.46, 0.2, 0.09, 5, GOLD);
        }
        Icon::Parks => {
            k.rect(0.45, 0.58, 0.55, 0.9, 0.02, BARK);
            k.circle(0.5, 0.36, 0.25, LEAF);
            k.circle(0.32, 0.52, 0.17, LEAF);
            k.circle(0.68, 0.52, 0.17, LEAF);
            k.circle(0.42, 0.3, 0.07, LEAF.gamma_multiply(1.3));
        }
        Icon::Landmark => {
            k.star(0.5, 0.5, 0.42, 0.18, 5, GOLD);
            k.ring(0.5, 0.5, 0.06, 0.04, INK);
        }
        Icon::Bulldoze => {
            k.line((0.22, 0.88), (0.6, 0.42), 0.1, BARK);
            k.poly(&[(0.4, 0.3), (0.6, 0.1), (0.9, 0.4), (0.7, 0.6)], STEEL);
            k.poly(&[(0.4, 0.3), (0.5, 0.2), (0.62, 0.32), (0.52, 0.42)], BAD);
        }
        Icon::DataViews => {
            let lens: Vec<(f32, f32)> = (0..=16)
                .map(|i| i as f32 / 16.0)
                .map(|t| (0.08 + 0.84 * t, 0.5 - 0.28 * (PI * t).sin()))
                .chain((1..16).rev().map(|i| i as f32 / 16.0).map(|t| (0.08 + 0.84 * t, 0.5 + 0.28 * (PI * t).sin())))
                .collect();
            k.poly(&lens, PAPER);
            k.path(&[lens.as_slice(), &lens[..1]].concat(), 0.06, INK);
            k.circle(0.5, 0.5, 0.18, SKY);
            k.circle(0.5, 0.5, 0.08, INK);
        }
        Icon::Budget | Icon::Coins => {
            for (i, x) in [0.34, 0.66].into_iter().enumerate() {
                let y = 0.58 - i as f32 * 0.1;
                k.circle(x, y, 0.26, GOLD);
                k.ring(x, y, 0.26, 0.05, INK);
                k.text(x, y, 0.32, "$", INK);
            }
        }
        Icon::City => {
            k.rect(0.08, 0.42, 0.36, 0.92, 0.02, BRICK);
            k.rect(0.36, 0.14, 0.66, 0.92, 0.02, NAVY);
            k.rect(0.66, 0.32, 0.92, 0.92, 0.02, GOOD);
            for (x, y0) in [(0.22, 0.52), (0.51, 0.24), (0.79, 0.42)] {
                for j in 0..3 {
                    k.rect(x - 0.05, y0 + j as f32 * 0.14, x + 0.05, y0 + j as f32 * 0.14 + 0.07, 0.0, ACCENT_SOFT);
                }
            }
        }
        Icon::Mood => {
            k.circle(0.5, 0.5, 0.4, GOLD);
            k.ring(0.5, 0.5, 0.4, 0.06, INK);
            k.circle(0.36, 0.4, 0.05, INK);
            k.circle(0.64, 0.4, 0.05, INK);
            k.arc(0.5, 0.48, 0.22, 0.5, PI - 0.5, 0.07, INK);
        }
        Icon::Gear => {
            for i in 0..8 {
                let a = i as f32 * TAU / 8.0;
                let (c, s) = (a.cos(), a.sin());
                let at = |r: f32, d: f32| (0.5 + r * c - d * s, 0.5 + r * s + d * c);
                k.poly(&[at(0.26, -0.08), at(0.44, -0.07), at(0.44, 0.07), at(0.26, 0.08)], INK_SOFT);
            }
            k.circle(0.5, 0.5, 0.3, INK_SOFT);
            k.circle(0.5, 0.5, 0.12, bg);
        }
        Icon::Walk => {
            k.circle(0.56, 0.16, 0.1, INK);
            k.path(&[(0.54, 0.3), (0.46, 0.6)], 0.11, INK);
            k.path(&[(0.46, 0.6), (0.3, 0.9)], 0.09, INK);
            k.path(&[(0.46, 0.6), (0.64, 0.74), (0.66, 0.92)], 0.09, INK);
            k.path(&[(0.3, 0.46), (0.52, 0.34), (0.72, 0.5)], 0.08, INK);
        }
        Icon::Pause => {
            k.rect(0.26, 0.2, 0.42, 0.8, 0.04, INK);
            k.rect(0.58, 0.2, 0.74, 0.8, 0.04, INK);
        }
        Icon::Play | Icon::Fast | Icon::Faster => {
            let n = match icon {
                Icon::Play => 1,
                Icon::Fast => 2,
                _ => 3,
            };
            let w = 0.7 / n as f32;
            for i in 0..n {
                let x = 0.15 + i as f32 * w + if n == 1 { 0.1 } else { 0.0 };
                let tip = if n == 1 { 0.62 } else { w * 1.05 };
                k.poly(&[(x, 0.22), (x + tip, 0.5), (x, 0.78)], INK);
            }
        }
        Icon::People => {
            for (x, c) in [(0.33, NAVY), (0.67, BRICK)] {
                k.circle(x, 0.32, 0.13, c);
                let body: Vec<(f32, f32)> = (0..=10).map(|i| PI + PI * i as f32 / 10.0).map(|a| (x + 0.2 * a.cos(), 0.84 + 0.3 * a.sin())).collect();
                k.poly(&body, c);
            }
        }
        Icon::Money => {
            k.rect(0.06, 0.24, 0.94, 0.76, 0.06, GOOD);
            k.frame(0.12, 0.3, 0.88, 0.7, 0.04, 0.04, PAPER);
            k.circle(0.5, 0.5, 0.13, PAPER);
            k.text(0.5, 0.5, 0.22, "$", GOOD);
        }
        Icon::Calendar => {
            k.rect(0.12, 0.2, 0.88, 0.88, 0.08, PAPER);
            k.rect(0.12, 0.2, 0.88, 0.38, 0.06, BRICK);
            k.frame(0.12, 0.2, 0.88, 0.88, 0.08, 0.06, INK);
            k.line((0.32, 0.1), (0.32, 0.28), 0.07, INK);
            k.line((0.68, 0.1), (0.68, 0.28), 0.07, INK);
            for i in 0..3 {
                for j in 0..2 {
                    k.rect(0.24 + i as f32 * 0.2, 0.48 + j as f32 * 0.18, 0.36 + i as f32 * 0.2, 0.58 + j as f32 * 0.18, 0.0, INK_SOFT);
                }
            }
        }
        Icon::Jobs => {
            k.frame(0.36, 0.14, 0.64, 0.34, 0.05, 0.07, INK);
            k.rect(0.1, 0.3, 0.9, 0.86, 0.08, BARK);
            k.rect(0.1, 0.5, 0.9, 0.56, 0.0, INK.gamma_multiply(0.6));
            k.rect(0.44, 0.46, 0.56, 0.6, 0.02, GOLD);
        }
        Icon::Sound | Icon::Muted => {
            k.poly(&[(0.1, 0.38), (0.28, 0.38), (0.5, 0.16), (0.5, 0.84), (0.28, 0.62), (0.1, 0.62)], INK);
            if icon == Icon::Sound {
                k.arc(0.5, 0.5, 0.18, -0.8, 0.8, 0.06, INK);
                k.arc(0.5, 0.5, 0.33, -0.8, 0.8, 0.06, INK);
            } else {
                k.line((0.62, 0.36), (0.88, 0.64), 0.08, BAD);
                k.line((0.88, 0.36), (0.62, 0.64), 0.08, BAD);
            }
        }
        Icon::Save => {
            k.rect(0.12, 0.12, 0.88, 0.88, 0.08, NAVY);
            k.rect(0.28, 0.12, 0.72, 0.36, 0.02, PAPER_DEEP);
            k.rect(0.58, 0.16, 0.66, 0.32, 0.0, NAVY);
            k.rect(0.24, 0.54, 0.76, 0.88, 0.02, PAPER);
        }
        Icon::Folder => {
            k.rect(0.08, 0.2, 0.46, 0.4, 0.05, GOLD.gamma_multiply(0.85));
            k.rect(0.08, 0.3, 0.92, 0.84, 0.06, GOLD);
            k.frame(0.08, 0.3, 0.92, 0.84, 0.06, 0.05, INK);
        }
        Icon::Sparkle => {
            k.star(0.42, 0.56, 0.36, 0.1, 4, GOLD);
            k.star(0.76, 0.24, 0.16, 0.05, 4, ACCENT);
        }
        Icon::Quit => {
            k.frame(0.14, 0.12, 0.56, 0.88, 0.04, 0.07, INK);
            k.line((0.42, 0.5), (0.88, 0.5), 0.08, BAD);
            k.path(&[(0.72, 0.34), (0.88, 0.5), (0.72, 0.66)], 0.08, BAD);
        }
        Icon::Help => {
            k.circle(0.5, 0.5, 0.4, SKY);
            k.text(0.5, 0.53, 0.56, "?", PAPER);
        }
        Icon::Leaf => {
            let leaf: Vec<(f32, f32)> = (0..24)
                .map(|i| i as f32 * TAU / 24.0)
                .map(|a| {
                    let (x, y) = (0.42 * a.cos(), 0.2 * a.sin());
                    let r = -PI / 4.0;
                    (0.5 + x * r.cos() - y * r.sin(), 0.5 + x * r.sin() + y * r.cos())
                })
                .collect();
            k.poly(&leaf, LEAF);
            k.line((0.24, 0.76), (0.72, 0.28), 0.05, PAPER);
        }
        Icon::Car => {
            k.poly(&[(0.28, 0.48), (0.36, 0.26), (0.64, 0.26), (0.74, 0.48)], BRICK.gamma_multiply(0.8));
            k.rect(0.08, 0.44, 0.92, 0.72, 0.08, BRICK);
            k.rect(0.38, 0.3, 0.48, 0.44, 0.0, ACCENT_SOFT);
            k.rect(0.54, 0.3, 0.64, 0.44, 0.0, ACCENT_SOFT);
            k.circle(0.28, 0.74, 0.1, INK);
            k.circle(0.72, 0.74, 0.1, INK);
        }
        Icon::Health => {
            k.circle(0.5, 0.5, 0.42, PAPER);
            k.ring(0.5, 0.5, 0.42, 0.05, BAD);
            k.rect(0.4, 0.2, 0.6, 0.8, 0.03, BAD);
            k.rect(0.2, 0.4, 0.8, 0.6, 0.03, BAD);
        }
        Icon::School => {
            k.poly(&[(0.08, 0.38), (0.5, 0.1), (0.92, 0.38)], BRICK);
            k.rect(0.14, 0.38, 0.86, 0.88, 0.02, PAPER_DEEP);
            for x in [0.26, 0.42, 0.58, 0.74] {
                k.rect(x - 0.04, 0.44, x + 0.04, 0.82, 0.0, INK_SOFT);
            }
            k.rect(0.1, 0.84, 0.9, 0.92, 0.0, INK);
        }
        Icon::Fire => {
            k.circle(0.5, 0.64, 0.26, BAD);
            k.poly(&[(0.5, 0.06), (0.26, 0.6), (0.74, 0.6)], BAD);
            k.circle(0.5, 0.7, 0.13, GOLD);
            k.poly(&[(0.5, 0.36), (0.38, 0.68), (0.62, 0.68)], GOLD);
        }
        Icon::Shop => {
            k.rect(0.14, 0.44, 0.86, 0.9, 0.02, PAPER_DEEP);
            for i in 0..5 {
                let x = 0.1 + i as f32 * 0.16;
                k.rect(x, 0.22, x + 0.16, 0.44, 0.0, if i % 2 == 0 { SKY } else { PAPER });
            }
            k.rect(0.24, 0.56, 0.48, 0.74, 0.0, SKY.gamma_multiply(0.6));
            k.rect(0.58, 0.56, 0.74, 0.9, 0.0, BARK);
        }
        Icon::Factory => {
            k.rect(0.7, 0.1, 0.82, 0.5, 0.0, BRICK);
            k.rect(0.08, 0.5, 0.92, 0.9, 0.02, STEEL);
            for i in 0..3 {
                let x = 0.08 + i as f32 * 0.21;
                k.poly(&[(x, 0.5), (x + 0.21, 0.3), (x + 0.21, 0.5)], STEEL);
            }
            k.rect(0.2, 0.62, 0.32, 0.74, 0.0, GOLD);
            k.rect(0.44, 0.62, 0.56, 0.74, 0.0, GOLD);
        }
        Icon::Office => {
            k.rect(0.26, 0.08, 0.74, 0.92, 0.02, NAVY);
            for j in 0..5 {
                for i in 0..3 {
                    let (x, y) = (0.33 + i as f32 * 0.13, 0.15 + j as f32 * 0.15);
                    k.rect(x, y, x + 0.08, y + 0.08, 0.0, ACCENT_SOFT);
                }
            }
        }
        Icon::House => {
            k.poly(&[(0.08, 0.5), (0.5, 0.12), (0.92, 0.5)], BRICK);
            k.rect(0.2, 0.48, 0.8, 0.9, 0.02, PAPER_DEEP);
            k.rect(0.42, 0.62, 0.58, 0.9, 0.0, BARK);
            k.rect(0.62, 0.56, 0.74, 0.68, 0.0, SKY);
        }
        Icon::Warning => {
            k.poly(&[(0.5, 0.08), (0.94, 0.88), (0.06, 0.88)], WARN);
            k.text(0.5, 0.6, 0.5, "!", PAPER);
        }
        Icon::Replay => {
            k.arc(0.5, 0.5, 0.32, -2.6, 2.2, 0.09, INK);
            k.poly(&[(0.06, 0.32), (0.3, 0.2), (0.3, 0.46)], INK);
            k.circle(0.5, 0.5, 0.06, INK);
        }
        Icon::Plant => {
            k.rect(0.08, 0.5, 0.7, 0.92, 0.02, STEEL);
            k.rect(0.74, 0.18, 0.88, 0.92, 0.0, BRICK);
            k.circle(0.8, 0.12, 0.08, PAPER_DEEP);
            k.poly(&[(0.3, 0.46), (0.42, 0.62), (0.34, 0.62), (0.42, 0.82), (0.22, 0.6), (0.3, 0.6)], GOLD);
        }
        Icon::Wind => {
            k.line((0.5, 0.42), (0.5, 0.94), 0.07, INK_SOFT);
            for i in 0..3 {
                let a = -PI / 2.0 + i as f32 * TAU / 3.0;
                k.line((0.5, 0.42), (0.5 + 0.38 * a.cos(), 0.42 + 0.38 * a.sin()), 0.08, PAPER_DEEP);
                k.line((0.5, 0.42), (0.5 + 0.38 * a.cos(), 0.42 + 0.38 * a.sin()), 0.03, INK);
            }
            k.circle(0.5, 0.42, 0.06, INK);
        }
        Icon::Sun => {
            for i in 0..8 {
                let a = i as f32 * TAU / 8.0;
                k.line((0.5 + 0.28 * a.cos(), 0.5 + 0.28 * a.sin()), (0.5 + 0.44 * a.cos(), 0.5 + 0.44 * a.sin()), 0.07, GOLD);
            }
            k.circle(0.5, 0.5, 0.22, GOLD);
        }
        Icon::Atom => {
            for i in 0..3 {
                let r = i as f32 * PI / 3.0;
                let pts: Vec<(f32, f32)> = (0..=24)
                    .map(|j| j as f32 * TAU / 24.0)
                    .map(|a| {
                        let (x, y) = (0.42 * a.cos(), 0.15 * a.sin());
                        (0.5 + x * r.cos() - y * r.sin(), 0.5 + x * r.sin() + y * r.cos())
                    })
                    .collect();
                k.path(&pts, 0.05, NAVY);
            }
            k.circle(0.5, 0.5, 0.08, BAD);
        }
        Icon::Tower => {
            k.path(&[(0.3, 0.92), (0.4, 0.5)], 0.06, INK);
            k.path(&[(0.7, 0.92), (0.6, 0.5)], 0.06, INK);
            k.rect(0.2, 0.12, 0.8, 0.5, 0.12, SKY);
            k.frame(0.2, 0.12, 0.8, 0.5, 0.12, 0.05, INK);
        }
        Icon::Cube => {
            let (t, l, r, b, c) = ((0.5, 0.08), (0.12, 0.3), (0.88, 0.3), (0.5, 0.92), (0.5, 0.52));
            k.poly(&[t, r, c, l], PAPER);
            k.poly(&[l, c, b, (0.12, 0.72)], ACCENT);
            k.poly(&[c, r, (0.88, 0.72), b], BORDER);
            k.path(&[t, r, (0.88, 0.72), b, (0.12, 0.72), l, t], 0.05, INK);
            k.path(&[l, c, r], 0.04, INK);
            k.line(c, b, 0.04, INK);
        }
        Icon::Mail => {
            k.rect(0.08, 0.22, 0.92, 0.78, 0.06, PAPER);
            k.poly(&[(0.08, 0.24), (0.92, 0.24), (0.5, 0.56)], PAPER_DEEP);
            k.path(&[(0.1, 0.26), (0.5, 0.56), (0.9, 0.26)], 0.05, INK);
            k.frame(0.08, 0.22, 0.92, 0.78, 0.06, 0.06, INK);
            k.circle(0.8, 0.3, 0.12, BAD);
        }
        Icon::Lock => {
            k.arc(0.5, 0.42, 0.2, PI, TAU, 0.09, INK_SOFT);
            k.line((0.3, 0.42), (0.3, 0.5), 0.09, INK_SOFT);
            k.line((0.7, 0.42), (0.7, 0.5), 0.09, INK_SOFT);
            k.rect(0.2, 0.48, 0.8, 0.9, 0.08, GOLD);
            k.circle(0.5, 0.64, 0.06, INK);
            k.line((0.5, 0.66), (0.5, 0.78), 0.05, INK);
        }
        Icon::Era => {
            k.rect(0.06, 0.62, 0.3, 0.92, 0.02, BRICK);
            k.poly(&[(0.04, 0.64), (0.18, 0.5), (0.32, 0.64)], BARK);
            k.rect(0.36, 0.4, 0.6, 0.92, 0.02, STEEL);
            k.rect(0.64, 0.1, 0.94, 0.92, 0.02, NAVY);
            for j in 0..4 {
                k.rect(0.7, 0.18 + j as f32 * 0.17, 0.88, 0.26 + j as f32 * 0.17, 0.0, ACCENT_SOFT);
            }
            k.star(0.2, 0.2, 0.14, 0.05, 4, GOLD);
        }
        Icon::Bus => {
            k.rect(0.08, 0.2, 0.92, 0.78, 0.1, GOLD);
            k.rect(0.14, 0.28, 0.86, 0.5, 0.04, NAVY);
            for x in [0.38f32, 0.62] {
                k.line((x, 0.28), (x, 0.5), 0.04, GOLD);
            }
            k.rect(0.08, 0.62, 0.92, 0.66, 0.0, INK_SOFT);
            k.circle(0.28, 0.8, 0.09, INK);
            k.circle(0.72, 0.8, 0.09, INK);
        }
        Icon::BusStop => {
            k.line((0.72, 0.1), (0.72, 0.92), 0.07, INK_SOFT);
            k.rect(0.56, 0.08, 0.9, 0.34, 0.06, GOLD);
            k.text(0.73, 0.22, 0.2, "B", INK);
            k.rect(0.08, 0.36, 0.56, 0.42, 0.02, BRICK);
            k.rect(0.1, 0.42, 0.14, 0.9, 0.0, INK_SOFT);
            k.rect(0.12, 0.44, 0.5, 0.84, 0.02, SKY.gamma_multiply(0.5));
            k.rect(0.16, 0.66, 0.5, 0.72, 0.0, BARK);
        }
        Icon::Ballot => {
            k.poly(&[(0.3, 0.06), (0.7, 0.06), (0.7, 0.46), (0.3, 0.46)], PAPER);
            k.frame(0.3, 0.06, 0.7, 0.46, 0.02, 0.04, INK_SOFT);
            k.path(&[(0.38, 0.26), (0.47, 0.36), (0.62, 0.16)], 0.06, GOOD);
            k.rect(0.1, 0.38, 0.9, 0.92, 0.06, NAVY);
            k.rect(0.26, 0.38, 0.74, 0.46, 0.0, INK);
            k.rect(0.1, 0.56, 0.9, 0.64, 0.0, ACCENT_SOFT);
        }
        Icon::Mayor => {
            k.rect(0.3, 0.06, 0.7, 0.34, 0.03, INK);
            k.rect(0.2, 0.3, 0.8, 0.38, 0.03, INK);
            k.rect(0.3, 0.26, 0.7, 0.3, 0.0, BRICK);
            k.circle(0.5, 0.5, 0.14, Color32::from_rgb(232, 196, 160));
            let body: Vec<(f32, f32)> = (0..=10).map(|i| PI + PI * i as f32 / 10.0).map(|a| (0.5 + 0.3 * a.cos(), 0.98 + 0.32 * a.sin())).collect();
            k.poly(&body, NAVY);
            k.line((0.32, 0.72), (0.62, 0.96), 0.08, GOLD);
        }
        Icon::Check => {
            k.circle(0.5, 0.5, 0.42, GOOD);
            k.path(&[(0.3, 0.52), (0.44, 0.66), (0.7, 0.36)], 0.1, PAPER);
        }
        Icon::Cross => {
            k.circle(0.5, 0.5, 0.42, BAD);
            k.line((0.34, 0.34), (0.66, 0.66), 0.1, PAPER);
            k.line((0.66, 0.34), (0.34, 0.66), 0.1, PAPER);
        }
        Icon::Plane => {
            k.poly(&[(0.46, 0.08), (0.54, 0.08), (0.56, 0.92), (0.44, 0.92)], STEEL);
            k.poly(&[(0.5, 0.36), (0.94, 0.6), (0.94, 0.66), (0.5, 0.52), (0.06, 0.66), (0.06, 0.6)], STEEL);
            k.poly(&[(0.5, 0.76), (0.7, 0.9), (0.3, 0.9)], STEEL);
        }
    }
}

/// Paint `icon` centred in `rect` at `size` points.
pub fn paint_in(p: &Painter, rect: Rect, size: f32, icon: Icon, bg: Color32) {
    paint(p, Rect::from_center_size(rect.center(), egui::vec2(size, size)), icon, bg);
}

/// An inline icon that sits on a line of text.
pub fn inline(ui: &mut egui::Ui, icon: Icon, size: f32) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    paint(ui.painter(), r, icon, ui.visuals().panel_fill);
    resp
}

/// A round badge with an icon, as on the tool rails. `label` goes underneath.
pub fn rail_button(ui: &mut egui::Ui, icon: Icon, label: &str, selected: bool) -> egui::Response {
    let w = 68.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 70.0), egui::Sense::click());
    let p = ui.painter();
    let c = egui::pos2(rect.center().x, rect.top() + 26.0);
    let fill = if selected {
        pal::ACCENT_SOFT
    } else if resp.hovered() {
        pal::PAPER
    } else {
        pal::PAPER_DIM
    };
    p.circle_filled(c + egui::vec2(0.0, 2.0), 23.0, pal::PAPER_DEEP);
    p.circle_filled(c, 23.0, fill);
    p.circle_stroke(c, 23.0, Stroke::new(if selected { 2.5 } else { 1.5 }, if selected { pal::BORDER } else { pal::INK_SOFT }));
    paint(p, Rect::from_center_size(c, egui::vec2(28.0, 28.0)), icon, fill);
    p.text(
        egui::pos2(rect.center().x, rect.bottom() - 9.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(13.0),
        pal::INK,
    );
    resp
}

/// A wide button with an icon on the left, for menus and lists.
pub fn menu_button(ui: &mut egui::Ui, icon: Icon, label: &str, width: f32, height: f32, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    let p = ui.painter();
    let fill = if selected {
        pal::ACCENT_SOFT
    } else if resp.hovered() {
        pal::PAPER
    } else {
        pal::PAPER_DIM
    };
    p.rect_filled(rect.translate(egui::vec2(0.0, 2.0)), 10.0, pal::PAPER_DEEP);
    p.rect_filled(rect, 10.0, fill);
    p.rect_stroke(rect, 10.0, Stroke::new(if selected { 2.0 } else { 1.0 }, if selected { pal::BORDER } else { pal::INK_SOFT.gamma_multiply(0.6) }), egui::StrokeKind::Inside);
    let isz = (height * 0.62).min(30.0);
    let ir = Rect::from_center_size(egui::pos2(rect.left() + 10.0 + isz / 2.0, rect.center().y), egui::vec2(isz, isz));
    paint(p, ir, icon, fill);
    p.text(
        egui::pos2(ir.right() + 10.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional((height * 0.42).clamp(13.0, 18.0)),
        pal::INK,
    );
    resp
}
