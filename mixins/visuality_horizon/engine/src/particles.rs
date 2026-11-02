//! Owned particle storage: bounded admission, 20 Hz fixed updates driven by
//! the game's own client tick, and interpolated extraction for drawing.

use crate::assets::{Anim, Atlas, Orientation};
use crate::config::{Family, HARD_MAX_ACTIVE, Priority};
use crate::diagnostics::Counters;

/// What an emitter asks for. Units: blocks, blocks per tick, ticks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnRequest {
    pub family: Family,
    pub anim: Anim,
    pub pos: [f64; 3],
    pub vel: [f32; 3],
    pub lifetime: f32,
    /// Quad edge length in blocks at birth and at death.
    pub size: [f32; 2],
    pub rgba: [u8; 4],
    pub rot: f32,
    pub spin: f32,
    /// Blocks per tick² downward.
    pub gravity: f32,
    /// Velocity kept per tick (0..1).
    pub drag: f32,
    /// Sideways sway amplitude in blocks per tick (feathers, wisps).
    pub flutter: f32,
    /// Lit by the world's light (debris, slime, water); else full bright.
    pub lit: bool,
    /// Who made it, for fair sharing of a full budget.
    pub source: u64,
    pub seed: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub pos: [f64; 3],
    pub prev: [f64; 3],
    pub vel: [f32; 3],
    pub age: f32,
    pub lifetime: f32,
    pub size: [f32; 2],
    pub rot: f32,
    pub prev_rot: f32,
    pub spin: f32,
    pub gravity: f32,
    pub drag: f32,
    pub flutter: f32,
    pub seed: u32,
    pub rgba: [u8; 4],
    pub anim: Anim,
    pub family: Family,
    pub lit: bool,
    /// Packed world light (sky << 4 | block) at the particle, refreshed each tick.
    pub light: u8,
}

/// What the renderer draws for one particle this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instance {
    pub pos: [f64; 3],
    pub half: f32,
    pub rot: f32,
    pub uv: [f32; 4],
    pub rgba: [u8; 4],
    pub orientation: Orientation,
}

pub struct Limits {
    pub max_active: u32,
    pub max_spawns_per_tick: u32,
    pub max_pending: u32,
    pub family_max: [u32; 8],
}

pub struct Pool {
    pub live: Vec<Particle>,
    pending: Vec<(SpawnRequest, u32)>,
    family_count: [u32; 8],
    order: Vec<usize>,
    source_seen: Vec<(u64, u32)>,
}

impl Default for Pool {
    fn default() -> Self {
        Self::new()
    }
}

impl Pool {
    pub fn new() -> Self {
        Pool {
            live: Vec::with_capacity(HARD_MAX_ACTIVE as usize),
            pending: Vec::with_capacity(256),
            family_count: [0; 8],
            order: Vec::with_capacity(256),
            source_seen: Vec::with_capacity(64),
        }
    }

    pub fn len(&self) -> usize {
        self.live.len()
    }
    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
    pub fn family_count(&self, f: Family) -> u32 {
        self.family_count[f.index()]
    }

    /// Queues a request; refuses invalid ones and drops when the queue is full.
    pub fn request(&mut self, r: SpawnRequest, limits: &Limits, c: &mut Counters) -> bool {
        if !valid(&r) {
            c.rejected_invalid += 1;
            return false;
        }
        if self.pending.len() >= limits.max_pending as usize {
            c.dropped_queue += 1;
            return false;
        }
        // Per-source ordinal, so a full tick admits one per source in turn.
        let ordinal = match self.source_seen.iter_mut().find(|(s, _)| *s == r.source) {
            Some((_, n)) => {
                *n += 1;
                *n
            }
            None => {
                if self.source_seen.len() < 1024 {
                    self.source_seen.push((r.source, 0));
                }
                0
            }
        };
        self.pending.push((r, ordinal));
        c.queue_high_water = c.queue_high_water.max(self.pending.len() as u32);
        true
    }

    /// Admits queued requests: feedback first, then contextual, then
    /// ambient; within a class one request per source in turn. A full
    /// budget lets feedback replace the oldest ambient particle; everything
    /// left over is dropped, never kept for a later burst.
    pub fn admit(&mut self, limits: &Limits, c: &mut Counters) {
        let max_active = limits.max_active.min(HARD_MAX_ACTIVE) as usize;
        self.order.clear();
        self.order.extend(0..self.pending.len());
        let pending = &self.pending;
        self.order.sort_by(|&a, &b| {
            let (ra, oa) = &pending[a];
            let (rb, ob) = &pending[b];
            rb.family
                .priority()
                .cmp(&ra.family.priority())
                .then(oa.cmp(ob))
                .then(a.cmp(&b))
        });
        let mut admitted = 0u32;
        for i in 0..self.order.len() {
            let (r, _) = self.pending[self.order[i]];
            if admitted >= limits.max_spawns_per_tick {
                c.dropped_budget += 1;
                continue;
            }
            if self.family_count[r.family.index()] >= limits.family_max[r.family.index()] {
                c.dropped_budget += 1;
                continue;
            }
            if self.live.len() >= max_active {
                if r.family.priority() == Priority::Feedback && self.evict_ambient() {
                    c.replaced += 1;
                } else {
                    c.dropped_budget += 1;
                    continue;
                }
            }
            self.live.push(Particle {
                pos: r.pos,
                prev: r.pos,
                vel: r.vel,
                age: 0.0,
                lifetime: r.lifetime,
                size: r.size,
                rot: r.rot,
                prev_rot: r.rot,
                spin: r.spin,
                gravity: r.gravity,
                drag: r.drag,
                flutter: r.flutter,
                seed: r.seed,
                rgba: r.rgba,
                anim: r.anim,
                family: r.family,
                lit: r.lit,
                light: 0xF0,
            });
            self.family_count[r.family.index()] += 1;
            admitted += 1;
            c.spawned += 1;
            c.family_spawned[r.family.index()] += 1;
        }
        self.pending.clear();
        self.source_seen.clear();
        c.active_high_water = c.active_high_water.max(self.live.len() as u32);
    }

    /// Removes the ambient particle closest to its end. False when none.
    fn evict_ambient(&mut self) -> bool {
        let victim = self
            .live
            .iter()
            .enumerate()
            .filter(|(_, p)| p.family.priority() == Priority::Ambient)
            .max_by(|(_, a), (_, b)| {
                (a.age / a.lifetime)
                    .partial_cmp(&(b.age / b.lifetime))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i);
        match victim {
            Some(i) => {
                let p = self.live.swap_remove(i);
                self.family_count[p.family.index()] -= 1;
                true
            }
            None => false,
        }
    }

    /// One 20 Hz step: move, age, expire. `light` reads the world's packed
    /// light at a block.
    pub fn tick(&mut self, mut light: impl FnMut([i32; 3]) -> u8, c: &mut Counters) {
        let mut i = 0;
        while i < self.live.len() {
            let p = &mut self.live[i];
            p.prev = p.pos;
            p.prev_rot = p.rot;
            p.age += 1.0;
            if p.age >= p.lifetime {
                let fam = p.family;
                self.live.swap_remove(i);
                self.family_count[fam.index()] -= 1;
                c.expired += 1;
                continue;
            }
            p.vel[1] -= p.gravity;
            for v in &mut p.vel {
                *v *= p.drag;
            }
            let mut dx = p.vel[0] as f64;
            let mut dz = p.vel[2] as f64;
            if p.flutter != 0.0 {
                // Deterministic sway from the particle's own age and seed.
                let phase = (p.seed % 628) as f32 / 100.0 + p.age * 0.45;
                dx += (p.flutter * phase.sin()) as f64;
                dz += (p.flutter * 0.6 * (phase * 0.7).cos()) as f64;
            }
            p.pos[0] += dx;
            p.pos[1] += p.vel[1] as f64;
            p.pos[2] += dz;
            p.rot += p.spin;
            if p.lit {
                p.light = light([
                    p.pos[0].floor() as i32,
                    p.pos[1].floor() as i32,
                    p.pos[2].floor() as i32,
                ]);
            }
            i += 1;
        }
    }

    pub fn clear(&mut self) {
        self.live.clear();
        self.pending.clear();
        self.source_seen.clear();
        self.family_count = [0; 8];
    }

    pub fn clear_family(&mut self, f: Family) {
        self.live.retain(|p| p.family != f);
        self.pending.retain(|(r, _)| r.family != f);
        self.family_count[f.index()] = 0;
    }

    /// Draw data at `partial` (0..1) ticks past the last step. `sky` scales
    /// sky light by the time of day (0..1).
    pub fn extract(&self, partial: f32, atlas: &Atlas, sky: f32, out: &mut Vec<Instance>) {
        out.clear();
        let t = partial.clamp(0.0, 1.0);
        for p in &self.live {
            let age = p.age + t;
            let phase = (age / p.lifetime).clamp(0.0, 1.0);
            let frame = atlas.animation(p.anim).frame(phase);
            let pos = [
                p.prev[0] + (p.pos[0] - p.prev[0]) * t as f64,
                p.prev[1] + (p.pos[1] - p.prev[1]) * t as f64,
                p.prev[2] + (p.pos[2] - p.prev[2]) * t as f64,
            ];
            let size = p.size[0] + (p.size[1] - p.size[0]) * phase;
            let mut rgba = p.rgba;
            // A short fade over the last fifth of life, so nothing pops out.
            if phase > 0.8 {
                rgba[3] = (rgba[3] as f32 * ((1.0 - phase) / 0.2)) as u8;
            }
            if p.lit {
                let b = brightness(p.light, sky);
                for ch in &mut rgba[..3] {
                    *ch = (*ch as f32 * b) as u8;
                }
            }
            out.push(Instance {
                pos,
                half: size * 0.5,
                rot: p.prev_rot + (p.rot - p.prev_rot) * t,
                uv: frame.uv,
                rgba,
                orientation: p.anim.orientation(),
            });
        }
    }
}

/// Vanilla-like light curve: max of block light and time-scaled sky light.
pub fn brightness(packed: u8, sky: f32) -> f32 {
    let block = (packed & 15) as f32;
    let skyl = (packed >> 4) as f32 * sky.clamp(0.0, 1.0);
    let level = block.max(skyl) / 15.0;
    // A gentle curve with a floor so particles never vanish into black.
    0.22 + 0.78 * (level / (4.0 - 3.0 * level))
}

fn valid(r: &SpawnRequest) -> bool {
    r.pos.iter().all(|v| v.is_finite() && v.abs() < 3.0e7)
        && r.vel.iter().all(|v| v.is_finite() && v.abs() < 4.0)
        && r.lifetime.is_finite()
        && r.lifetime >= 1.0
        && r.lifetime <= 200.0
        && r.size
            .iter()
            .all(|s| s.is_finite() && *s > 0.0 && *s <= 2.0)
        && r.rot.is_finite()
        && r.spin.is_finite()
        && r.gravity.is_finite()
        && r.drag.is_finite()
        && (0.0..=1.0).contains(&r.drag)
        && r.flutter.is_finite()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn req(family: Family, source: u64) -> SpawnRequest {
        SpawnRequest {
            family,
            anim: Anim::Sparkle,
            pos: [0.5, 64.0, 0.5],
            vel: [0.0; 3],
            lifetime: 10.0,
            size: [0.1, 0.1],
            rgba: [255; 4],
            rot: 0.0,
            spin: 0.0,
            gravity: 0.0,
            drag: 1.0,
            flutter: 0.0,
            lit: false,
            source,
            seed: 1,
        }
    }

    pub fn limits(active: u32, spawns: u32) -> Limits {
        Limits {
            max_active: active,
            max_spawns_per_tick: spawns,
            max_pending: 256,
            family_max: [1024; 8],
        }
    }

    #[test]
    fn lim_01_global_cap_is_never_exceeded() {
        let mut pool = Pool::new();
        let mut c = Counters::default();
        let l = limits(128, 1000);
        for tick in 0..50 {
            for s in 0..300 {
                pool.request(req(Family::Ores, s), &l, &mut c);
            }
            pool.admit(&l, &mut c);
            assert!(pool.len() <= 128, "tick {tick}: {}", pool.len());
            pool.tick(|_| 0xF0, &mut c);
        }
    }

    #[test]
    fn lim_02_queue_is_bounded_and_leftovers_dropped() {
        let mut pool = Pool::new();
        let mut c = Counters::default();
        let mut l = limits(1024, 24);
        l.max_pending = 100;
        for s in 0..500 {
            pool.request(req(Family::Water, s), &l, &mut c);
        }
        assert_eq!(pool.pending_len(), 100);
        assert_eq!(c.dropped_queue, 400);
        pool.admit(&l, &mut c);
        assert_eq!(pool.len(), 24);
        assert_eq!(pool.pending_len(), 0);
        // Nothing comes back later as a delayed burst.
        pool.admit(&l, &mut c);
        assert_eq!(pool.len(), 24);
    }

    #[test]
    fn lim_03_feedback_first_and_fair_between_sources() {
        let mut pool = Pool::new();
        let mut c = Counters::default();
        let l = limits(1024, 4);
        for _ in 0..10 {
            pool.request(req(Family::Ores, 1), &l, &mut c);
        }
        for _ in 0..10 {
            pool.request(req(Family::Ores, 2), &l, &mut c);
        }
        pool.request(req(Family::Hit, 3), &l, &mut c);
        pool.admit(&l, &mut c);
        let fams: Vec<Family> = pool.live.iter().map(|p| p.family).collect();
        assert_eq!(fams[0], Family::Hit);
        let ores = pool.live.len() - 1;
        assert_eq!(ores, 3);
        // Sources 1 and 2 alternate rather than source 1 taking every slot.
        assert!(pool.family_count(Family::Ores) == 3);
    }

    #[test]
    fn feedback_replaces_oldest_ambient_when_full() {
        let mut pool = Pool::new();
        let mut c = Counters::default();
        let l = limits(4, 100);
        for s in 0..4 {
            pool.request(req(Family::Soul, s), &l, &mut c);
        }
        pool.admit(&l, &mut c);
        pool.request(req(Family::Water, 9), &l, &mut c);
        pool.request(req(Family::Hit, 10), &l, &mut c);
        pool.admit(&l, &mut c);
        assert_eq!(pool.len(), 4);
        assert_eq!(pool.family_count(Family::Hit), 1);
        assert_eq!(pool.family_count(Family::Water), 0);
        assert_eq!(c.replaced, 1);
    }

    #[test]
    fn family_ceiling_applies() {
        let mut pool = Pool::new();
        let mut c = Counters::default();
        let mut l = limits(1024, 100);
        l.family_max[Family::Charge.index()] = 3;
        for s in 0..10 {
            pool.request(req(Family::Charge, s), &l, &mut c);
        }
        pool.admit(&l, &mut c);
        assert_eq!(pool.family_count(Family::Charge), 3);
    }

    #[test]
    fn invalid_requests_are_refused() {
        let mut pool = Pool::new();
        let mut c = Counters::default();
        let l = limits(10, 10);
        let mut r = req(Family::Hit, 0);
        r.pos[0] = f64::NAN;
        assert!(!pool.request(r, &l, &mut c));
        let mut r = req(Family::Hit, 0);
        r.lifetime = 0.0;
        assert!(!pool.request(r, &l, &mut c));
        let mut r = req(Family::Hit, 0);
        r.size = [5.0, 5.0];
        assert!(!pool.request(r, &l, &mut c));
        assert_eq!(c.rejected_invalid, 3);
    }

    #[test]
    fn particles_expire_at_their_lifetime_and_counts_follow() {
        let mut pool = Pool::new();
        let mut c = Counters::default();
        let l = limits(10, 10);
        let mut r = req(Family::Hit, 0);
        r.lifetime = 3.0;
        pool.request(r, &l, &mut c);
        pool.admit(&l, &mut c);
        for _ in 0..2 {
            pool.tick(|_| 0, &mut c);
        }
        assert_eq!(pool.len(), 1);
        pool.tick(|_| 0, &mut c);
        assert_eq!(pool.len(), 0);
        assert_eq!(pool.family_count(Family::Hit), 0);
    }

    #[test]
    fn flutter_stays_bounded() {
        let mut pool = Pool::new();
        let mut c = Counters::default();
        let l = limits(10, 10);
        let mut r = req(Family::Hit, 0);
        r.lifetime = 200.0;
        r.flutter = 0.03;
        r.gravity = 0.002;
        r.drag = 0.9;
        pool.request(r, &l, &mut c);
        pool.admit(&l, &mut c);
        for _ in 0..199 {
            pool.tick(|_| 0, &mut c);
        }
        let p = pool.live[0];
        assert!((p.pos[0] - 0.5).abs() < 2.0, "{:?}", p.pos);
        assert!((p.pos[2] - 0.5).abs() < 2.0, "{:?}", p.pos);
    }

    #[test]
    fn clearing_a_family_keeps_the_others() {
        let mut pool = Pool::new();
        let mut c = Counters::default();
        let l = limits(10, 10);
        pool.request(req(Family::Hit, 0), &l, &mut c);
        pool.request(req(Family::Soul, 1), &l, &mut c);
        pool.admit(&l, &mut c);
        pool.clear_family(Family::Soul);
        assert_eq!(pool.len(), 1);
        assert_eq!(pool.family_count(Family::Soul), 0);
        assert_eq!(pool.family_count(Family::Hit), 1);
    }

    #[test]
    fn brightness_has_a_floor_and_full_light_is_one() {
        assert!((brightness(0xF0, 1.0) - 1.0).abs() < 1e-6);
        assert!(brightness(0x00, 1.0) >= 0.22);
        assert!(brightness(0xF0, 0.2) < brightness(0xF0, 1.0));
        assert!((brightness(0x0F, 0.0) - 1.0).abs() < 1e-6);
    }
}
