//! Tiny procedural sound effects, synthesized into WAV bytes at startup so the
//! game ships with no asset files.

use std::sync::Arc;

use bevy::audio::{AudioSource, PlaybackSettings, Volume};
use bevy::prelude::*;

use crate::settings::Settings;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sfx {
    Place,
    Bulldoze,
    Error,
    Cash,
    Join,
    GameOver,
    Fire,
}

#[derive(Resource, Default)]
pub struct SfxQueue {
    queue: Vec<Sfx>,
}

impl SfxQueue {
    pub fn push(&mut self, s: Sfx) {
        if !self.queue.contains(&s) {
            self.queue.push(s);
        }
    }
}

#[derive(Resource)]
struct SfxBank(Vec<(Sfx, Handle<AudioSource>)>);

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SfxQueue>().add_systems(Startup, build_bank).add_systems(Update, play);
    }
}

const RATE: u32 = 22_050;

/// Sweep from `f0` to `f1` Hz over `dur` seconds with an exponential decay.
/// `kind`: 0 = sine, 1 = square, 2 = noise.
fn tone(out: &mut Vec<f32>, f0: f32, f1: f32, dur: f32, kind: u8, vol: f32) {
    let n = (dur * RATE as f32) as usize;
    let mut phase = 0.0f32;
    let mut seed = 0x1234_5678u32;
    for i in 0..n {
        let t = i as f32 / n as f32;
        let f = f0 + (f1 - f0) * t;
        phase += f / RATE as f32;
        let s = match kind {
            0 => (phase * std::f32::consts::TAU).sin(),
            1 => {
                if phase.fract() < 0.5 {
                    0.6
                } else {
                    -0.6
                }
            }
            _ => {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed as f32 / u32::MAX as f32) * 2.0 - 1.0
            }
        };
        let attack = (i as f32 / (RATE as f32 * 0.005)).min(1.0);
        let env = attack * (1.0 - t).powf(2.0);
        out.push(s * env * vol);
    }
}

fn wav(samples: &[f32]) -> Vec<u8> {
    let data_len = samples.len() as u32 * 2;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
    }
    b
}

fn synth(s: Sfx) -> Vec<f32> {
    let mut v = Vec::new();
    match s {
        Sfx::Place => tone(&mut v, 660.0, 880.0, 0.07, 0, 0.5),
        Sfx::Bulldoze => tone(&mut v, 200.0, 80.0, 0.18, 2, 0.35),
        Sfx::Error => tone(&mut v, 160.0, 120.0, 0.16, 1, 0.25),
        Sfx::Cash => {
            tone(&mut v, 988.0, 988.0, 0.06, 0, 0.3);
            tone(&mut v, 1319.0, 1319.0, 0.12, 0, 0.3);
        }
        Sfx::Join => {
            tone(&mut v, 523.0, 523.0, 0.08, 0, 0.4);
            tone(&mut v, 784.0, 784.0, 0.12, 0, 0.4);
        }
        Sfx::GameOver => {
            for f in [523.0, 659.0, 784.0, 1047.0] {
                tone(&mut v, f, f, 0.16, 0, 0.45);
            }
        }
        Sfx::Fire => {
            tone(&mut v, 880.0, 660.0, 0.12, 1, 0.22);
            tone(&mut v, 880.0, 660.0, 0.12, 1, 0.22);
            tone(&mut v, 300.0, 120.0, 0.35, 2, 0.3);
        }
    }
    v
}

fn build_bank(mut commands: Commands, mut sources: ResMut<Assets<AudioSource>>) {
    let all = [Sfx::Place, Sfx::Bulldoze, Sfx::Error, Sfx::Cash, Sfx::Join, Sfx::GameOver, Sfx::Fire];
    let bank = all
        .into_iter()
        .map(|s| (s, sources.add(AudioSource { bytes: Arc::from(wav(&synth(s))) })))
        .collect();
    commands.insert_resource(SfxBank(bank));
}

fn play(mut commands: Commands, mut q: ResMut<SfxQueue>, bank: Option<Res<SfxBank>>, settings: Res<Settings>) {
    let Some(bank) = bank else { return };
    let items = std::mem::take(&mut q.queue);
    if settings.muted || settings.volume <= 0.0 {
        return;
    }
    for s in items {
        if let Some((_, h)) = bank.0.iter().find(|(k, _)| *k == s) {
            commands.spawn((AudioPlayer::new(h.clone()), PlaybackSettings::DESPAWN.with_volume(Volume::Linear(0.8 * settings.volume))));
        }
    }
}
