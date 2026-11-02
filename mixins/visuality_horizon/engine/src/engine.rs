//! The effect runtime: lifecycle, identities and cooldowns, the eight
//! emitters, and the owned particle pool. Plain Rust in and out: the game
//! glue copies what it observes into the records below.

use crate::assets::{Anim, Atlas};
use crate::config::{Config, Family, HitEffect, MinimalMode};
use crate::diagnostics::Counters;
use crate::particles::{Instance, Limits, Pool, SpawnRequest};
use crate::rng::Rng;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub const MOD_ID: &str = "visuality_horizon";

/// Block classes the ambient emitters look for.
pub mod class {
    pub const NONE: u8 = 0;
    pub const ORE: u8 = 1;
    pub const AMETHYST: u8 = 2;
    pub const SOUL: u8 = 4;
    pub const UNKNOWN: u8 = 0xFF;
}
/// The largest block state id the class table grows to.
const MAX_STATE_ID: u32 = 1 << 20;
/// Longest a cooldown entry lives without its entity being removed.
const COOLDOWN_CAP: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lifecycle {
    /// Compiled in, master switch or the launcher's Mods toggle off.
    DisabledReady,
    /// Ready, no world session.
    ReadyNoWorld,
    /// A world is open: events produce effects.
    ActiveWorld,
    /// The art could not be prepared: no drawing, no emission, one error.
    VisualFailure,
}

/// One visible living entity, copied from the client each tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct EntityObs<'a> {
    pub id: i32,
    pub kind: &'a str,
    pub pos: [f64; 3],
    pub width: f32,
    pub height: f32,
    pub baby: bool,
    pub crouching: bool,
    pub invisible: bool,
    pub alive: bool,
    pub on_ground: bool,
    pub sprinting: bool,
    /// The creeper is powered (meaningless for other kinds).
    pub charged: bool,
    /// Chicken variant index: 0 temperate, 1 warm, 2 cold.
    pub variant: u8,
    /// Head, chest, legs, feet item ids.
    pub armor: [Option<&'a str>; 4],
    /// The block the entity stands on is in the soul class.
    pub on_soul_block: bool,
    /// This is the local player.
    pub local: bool,
}

/// One water column sampled for rain ripples.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterColumn {
    /// The y of the water surface to draw on.
    pub surface_y: f64,
    /// The top block is still, full source water.
    pub source: bool,
    /// Rain falls on it (biome precipitation is rain, nothing above it).
    pub raining: bool,
    /// The biome's water colour, 0xRRGGBB.
    pub water_rgb: u32,
}

/// What the client says at the start of a tick.
#[derive(Clone, Copy, Debug)]
pub struct TickInfo<'a> {
    pub native_on: bool,
    pub in_world: bool,
    pub paused: bool,
    pub dimension: &'a str,
    pub camera: [f64; 3],
    pub first_person: bool,
    /// The game's Particles option: 0 all, 1 decreased, 2 minimal.
    pub particle_level: u8,
    /// Rain strength 0..1 (0 outside the overworld).
    pub rain: f32,
    /// Sky light scale by time of day 0..1.
    pub sky: f32,
    pub player_underwater: bool,
}

struct Resolved {
    hit: HashMap<String, HitEffect>,
    armor: HashSet<String>,
    armor_tags: Vec<String>,
    ore_names: HashSet<String>,
    soul_names: HashSet<String>,
}

pub struct Engine {
    pub config: Config,
    pub counters: Counters,
    pool: Pool,
    rng: Rng,
    state: Lifecycle,
    native_on: bool,
    session: u64,
    tick: u64,
    camera: [f64; 3],
    first_person: bool,
    particle_level: u8,
    rain: f32,
    sky: f32,
    dimension: String,
    hurts: Vec<(i32, bool, bool)>,
    cooldown: HashMap<i32, u64>,
    classes: Vec<u8>,
    resolved: Resolved,
    /// Item ids answered for armor tags, filled lazily.
    armor_cache: HashMap<Arc<str>, bool>,
    instances: Vec<Instance>,
    atlas: Option<Arc<Atlas>>,
    pub failure: Option<String>,
}

impl Engine {
    pub fn new(config: Config) -> Self {
        let mut e = Engine {
            config,
            counters: Counters::default(),
            pool: Pool::new(),
            rng: Rng::new(0x5649_5355),
            state: Lifecycle::ReadyNoWorld,
            native_on: true,
            session: 0,
            tick: 0,
            camera: [0.0; 3],
            first_person: true,
            particle_level: 0,
            rain: 0.0,
            sky: 1.0,
            dimension: String::new(),
            hurts: Vec::with_capacity(32),
            cooldown: HashMap::new(),
            classes: Vec::new(),
            resolved: Resolved {
                hit: HashMap::new(),
                armor: HashSet::new(),
                armor_tags: Vec::new(),
                ore_names: HashSet::new(),
                soul_names: HashSet::new(),
            },
            armor_cache: HashMap::new(),
            instances: Vec::with_capacity(crate::config::HARD_MAX_ACTIVE as usize),
            atlas: None,
            failure: None,
        };
        e.resolve_static();
        e.update_state(false);
        e
    }

    pub fn state(&self) -> Lifecycle {
        self.state
    }
    pub fn session(&self) -> u64 {
        self.session
    }
    pub fn active_len(&self) -> usize {
        self.pool.len()
    }
    pub fn pool(&self) -> &Pool {
        &self.pool
    }
    pub fn set_atlas(&mut self, atlas: Arc<Atlas>) {
        self.atlas = Some(atlas);
    }
    pub fn atlas(&self) -> Option<&Arc<Atlas>> {
        self.atlas.as_ref()
    }

    /// The art can't be drawn: stop everything, keep one message.
    pub fn visual_failure(&mut self, message: String) {
        if self.failure.is_none() {
            self.failure = Some(message);
        }
        self.pool.clear();
        self.hurts.clear();
        self.state = Lifecycle::VisualFailure;
    }

    fn running(&self) -> bool {
        self.state == Lifecycle::ActiveWorld
    }

    fn update_state(&mut self, in_world: bool) {
        let before = self.state;
        self.state = if self.failure.is_some() {
            Lifecycle::VisualFailure
        } else if !self.config.enabled || !self.native_on {
            Lifecycle::DisabledReady
        } else if in_world {
            Lifecycle::ActiveWorld
        } else {
            Lifecycle::ReadyNoWorld
        };
        if before == Lifecycle::ActiveWorld && self.state != Lifecycle::ActiveWorld {
            self.pool.clear();
            self.hurts.clear();
        }
    }

    /// A new world, server or dimension: nothing carries over.
    pub fn new_session(&mut self) {
        self.session += 1;
        self.counters.sessions += 1;
        self.pool.clear();
        self.hurts.clear();
        self.cooldown.clear();
        self.classes.clear();
        self.armor_cache.clear();
    }

    /// An entity id was added or removed: its cooldown is not inherited.
    pub fn entity_changed(&mut self, id: i32) {
        self.cooldown.remove(&id);
        self.hurts.retain(|h| h.0 != id);
    }

    /// Config changed: re-resolve lists, drop effects of families now off.
    pub fn config_changed(&mut self) {
        self.config.clamp();
        self.resolve_static();
        self.classes.clear();
        self.armor_cache.clear();
        for f in Family::ALL {
            if !self.config.family(f).enabled {
                self.pool.clear_family(f);
            }
        }
        if !self.config.enabled {
            self.pool.clear();
        }
        self.update_state(self.state == Lifecycle::ActiveWorld);
    }

    fn resolve_static(&mut self) {
        let c = &self.config;
        self.resolved.hit = c
            .hit_rules
            .iter()
            .map(|r| (r.entity.clone(), r.effect))
            .collect();
        self.resolved.armor = c
            .armor_items
            .iter()
            .filter(|i| !i.starts_with('#'))
            .cloned()
            .collect();
        self.resolved.armor_tags = c
            .armor_items
            .iter()
            .filter_map(|i| i.strip_prefix('#'))
            .map(str::to_owned)
            .collect();
    }

    /// Resolves block tags against the world's tags (`members(tag)` lists a
    /// tag's block ids, None when the tag is unknown). Returns the unknown tags.
    pub fn resolve_blocks(
        &mut self,
        mut members: impl FnMut(&str) -> Option<Vec<String>>,
    ) -> Vec<String> {
        let mut unknown = Vec::new();
        self.resolved.ore_names.clear();
        // Each entry is a block tag (`minecraft:gold_ores` or `#...`); an
        // entry without `#` that no tag has is taken as a block id.
        for entry in self.config.ore_tags.clone() {
            let tag = entry.strip_prefix('#').unwrap_or(&entry);
            match members(tag) {
                Some(names) => self.resolved.ore_names.extend(names),
                None if !entry.starts_with('#') => {
                    self.resolved.ore_names.insert(entry.clone());
                }
                None => unknown.push(entry.clone()),
            }
        }
        self.resolved.soul_names = members("minecraft:wither_summon_base_blocks")
            .map(|v| v.into_iter().collect())
            .unwrap_or_else(|| {
                ["minecraft:soul_sand", "minecraft:soul_soil"]
                    .map(String::from)
                    .into_iter()
                    .collect()
            });
        self.classes.clear();
        unknown
    }

    /// The ambient class of block state `state`, asking for its name once.
    pub fn block_class(&mut self, state: u32, name: impl FnOnce() -> Arc<str>) -> u8 {
        if state >= MAX_STATE_ID {
            return class::NONE;
        }
        let i = state as usize;
        if let Some(&c) = self.classes.get(i)
            && c != class::UNKNOWN
        {
            return c;
        }
        let name = name();
        let mut c = class::NONE;
        if self.resolved.ore_names.contains(&*name) {
            c |= class::ORE | (material_index(&name) << 4);
        }
        if &*name == "minecraft:amethyst_cluster"
            || (!self.config.amethyst_full_clusters_only && name.ends_with("_amethyst_bud"))
        {
            c |= class::AMETHYST;
        }
        if self.resolved.soul_names.contains(&*name) {
            c |= class::SOUL;
        }
        if self.classes.len() <= i {
            self.classes.resize(i + 1, class::UNKNOWN);
        }
        self.classes[i] = c;
        c
    }

    /// Whether `item` is configured shiny armor (`in_tag` answers item tags).
    pub fn shiny(&mut self, item: &str, mut in_tag: impl FnMut(&str, &str) -> bool) -> bool {
        if self.resolved.armor.contains(item) {
            return true;
        }
        if self.resolved.armor_tags.is_empty() {
            return false;
        }
        if let Some(&b) = self.armor_cache.get(item) {
            return b;
        }
        let b = self.resolved.armor_tags.iter().any(|t| in_tag(t, item));
        if self.armor_cache.len() < 4096 {
            self.armor_cache.insert(Arc::from(item), b);
        }
        b
    }

    // ----- per-tick flow -------------------------------------------------

    /// Start of a client tick, before entities and samples.
    pub fn begin_tick(&mut self, info: &TickInfo<'_>) {
        self.native_on = info.native_on;
        if info.dimension != self.dimension {
            self.dimension.clear();
            self.dimension.push_str(info.dimension);
            self.new_session();
        }
        self.update_state(info.in_world);
        if info.paused {
            return;
        }
        self.tick += 1;
        self.camera = info.camera;
        self.first_person = info.first_person;
        self.particle_level = info.particle_level;
        self.rain = info.rain;
        self.sky = info.sky;
    }

    /// The family's chance multiplier now: 0 when it must not emit.
    fn scale(&mut self, f: Family) -> f32 {
        if !self.running() {
            return 0.0;
        }
        let c = &self.config;
        let fam = c.family(f);
        if !fam.enabled {
            return 0.0;
        }
        let mut s = fam.intensity * c.quality.global_intensity;
        if c.quality.respect_game_particle_setting {
            match self.particle_level {
                0 => {}
                1 => s *= 2.0 / 3.0,
                _ => match c.quality.minimal_mode {
                    MinimalMode::NoExtraParticles => s = 0.0,
                    MinimalMode::FeedbackOnly => {
                        if f.priority() != crate::config::Priority::Feedback {
                            s = 0.0
                        }
                    }
                },
            }
        }
        if s <= 0.0 {
            self.counters.rejected_quality += 1;
        }
        s
    }

    /// Squared distance from the camera beyond which an entity can't emit
    /// anything (the emission distance plus a body's reach), so the glue can
    /// skip copying it at all.
    pub fn entity_reach_sq(&self) -> f64 {
        let d = self.config.quality.emission_distance_blocks as f64 + 3.0;
        d * d
    }

    fn near(&mut self, p: [f64; 3]) -> bool {
        let d = self.config.quality.emission_distance_blocks as f64;
        let c = self.camera;
        let ok = (p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2) <= d * d;
        if !ok {
            self.counters.rejected_distance += 1;
        }
        ok
    }

    fn limits(&self) -> Limits {
        let q = &self.config.quality;
        Limits {
            max_active: q.max_active_particles,
            max_spawns_per_tick: q.max_spawns_per_tick,
            max_pending: q.max_pending_requests,
            family_max: Family::ALL.map(|f| self.config.family(f).max_active),
        }
    }

    fn push(&mut self, r: SpawnRequest) {
        let l = self.limits();
        self.pool.request(r, &l, &mut self.counters);
    }

    fn source(&self, kind: u64, id: i64) -> u64 {
        (self.session << 48) ^ (kind << 40) ^ (id as u64)
    }

    /// A hurt signal from the client's packets. `canonical` is the damage
    /// event; `attacker` says the damage named a source entity.
    pub fn hurt_signal(&mut self, id: i32, canonical: bool, attacker: bool) {
        if !canonical {
            self.counters.hurt_noncanonical += 1;
            return;
        }
        if self.hurts.len() < 64 {
            self.hurts.push((id, canonical, attacker));
        }
    }

    /// One visible entity this tick.
    pub fn entity(&mut self, e: &EntityObs<'_>, in_tag: &mut dyn FnMut(&str, &str) -> bool) {
        if !self.running() {
            return;
        }
        if let Some(i) = self.hurts.iter().position(|h| h.0 == e.id) {
            let (_, _, attacker) = self.hurts.swap_remove(i);
            self.counters.hurt_canonical += 1;
            if attacker && e.alive {
                self.hit(e);
            }
        }
        if !e.alive || e.invisible {
            return;
        }
        if e.charged && e.kind == "minecraft:creeper" {
            self.charge(e);
        }
        if !(e.local && self.first_person && !self.config.armor_show_local_first_person) {
            self.armor(e, in_tag);
        }
        if e.sprinting && e.on_ground && e.on_soul_block && self.config.soul_sprinting {
            self.soul_sprint(e);
        }
    }

    /// End of the entity pass: drop hurts whose entity wasn't seen.
    pub fn end_entities(&mut self) {
        self.hurts.clear();
        if self.cooldown.len() > COOLDOWN_CAP {
            let now = self.tick;
            let window = self.config.hit_cooldown_ticks as u64;
            self.cooldown
                .retain(|_, t| now.saturating_sub(*t) <= window);
        }
    }

    fn hit(&mut self, e: &EntityObs<'_>) {
        let Some(&effect) = self.resolved.hit.get(e.kind) else {
            self.counters.hurt_unmapped += 1;
            return;
        };
        let window = self.config.hit_cooldown_ticks as u64;
        if let Some(&last) = self.cooldown.get(&e.id)
            && self.tick.saturating_sub(last) < window
        {
            self.counters.hurt_throttled += 1;
            return;
        }
        let s = self.scale(Family::Hit);
        if s <= 0.0 || !self.near(e.pos) {
            return;
        }
        self.cooldown.insert(e.id, self.tick);
        let effect = match effect {
            HitEffect::FeatherByVariant => match e.variant {
                1 => HitEffect::WarmFeather,
                2 => HitEffect::ColdFeather,
                _ => HitEffect::Feather,
            },
            x => x,
        };
        let base = (2 + self.rng.below(3)) as f32; // 2..=4
        let count = ((base * s).round() as u32).clamp(1, self.config.hit_max_fragments);
        let w = e.width.clamp(0.2, 4.0) as f64;
        let h = e.height.clamp(0.2, 4.0) as f64;
        let source = self.source(1, e.id as i64);
        let reduced = self.config.reduced_motion;
        for _ in 0..count {
            let r = &mut self.rng;
            let pos = [
                e.pos[0] + (r.f64() - 0.5) * w * 0.6,
                e.pos[1] + h * (0.25 + 0.6 * r.f64()),
                e.pos[2] + (r.f64() - 0.5) * w * 0.6,
            ];
            let dir = r.f32() * std::f32::consts::TAU;
            let (anim, size, life, speed, up, gravity, drag, flutter, spin) = match effect {
                HitEffect::Bone | HitEffect::WitherBone => {
                    let a = match (effect, r.below(2)) {
                        (HitEffect::Bone, 0) => Anim::BoneA,
                        (HitEffect::Bone, _) => Anim::BoneB,
                        (_, 0) => Anim::WitherBoneA,
                        _ => Anim::WitherBoneB,
                    };
                    (
                        a,
                        r.range(0.18, 0.28),
                        r.range(7.0, 13.0),
                        r.range(0.04, 0.09),
                        r.range(0.08, 0.14),
                        0.035,
                        0.96,
                        0.0,
                        r.range(-0.35, 0.35),
                    )
                }
                HitEffect::Emerald => (
                    Anim::Emerald,
                    r.range(0.16, 0.24),
                    r.range(6.0, 10.0),
                    r.range(0.03, 0.07),
                    r.range(0.09, 0.15),
                    0.04,
                    0.96,
                    0.0,
                    r.range(-0.3, 0.3),
                ),
                _ => {
                    let a = match effect {
                        HitEffect::WarmFeather => Anim::FeatherWarm,
                        HitEffect::ColdFeather => Anim::FeatherCold,
                        _ => Anim::Feather,
                    };
                    (
                        a,
                        r.range(0.22, 0.34),
                        r.range(11.0, 20.0),
                        r.range(0.02, 0.05),
                        r.range(0.04, 0.08),
                        0.006,
                        0.9,
                        r.range(0.008, 0.016),
                        r.range(-0.12, 0.12),
                    )
                }
            };
            let calm = if reduced { 0.4 } else { 1.0 };
            let seed = r.next_u32();
            self.push(SpawnRequest {
                family: Family::Hit,
                anim,
                pos,
                vel: [dir.cos() * speed, up, dir.sin() * speed],
                lifetime: life,
                size: [size, size * 0.85],
                rgba: [255; 4],
                rot: dir,
                spin: spin * calm,
                gravity,
                drag,
                flutter: flutter * calm,
                lit: true,
                source,
                seed,
            });
        }
    }

    /// A normal slime landed. True: the mod draws its blobs, so the game's
    /// own slime crumbs are skipped. False: the game's own path runs.
    pub fn slime_landed(&mut self, id: i32, feet: [f64; 3], size: i32, width: f32) -> bool {
        if !self.running() || !self.config.family(Family::Slime).enabled {
            return false;
        }
        self.counters.slime_landings += 1;
        let s = self.scale(Family::Slime);
        // Replacement is decided by the switches alone, so the look never
        // flips between our blobs and vanilla crumbs as budgets fill.
        if s <= 0.0 || !self.near(feet) {
            return true;
        }
        let (anim, quad, base) = match size {
            ..=1 => (Anim::SlimeSmall, 0.18, 2.0),
            2..=3 => (Anim::SlimeMedium, 0.30, 3.0),
            _ => (Anim::SlimeLarge, 0.42, 4.0),
        };
        let count = ((base + self.rng.below(2) as f32) * s)
            .round()
            .clamp(1.0, 8.0) as u32;
        let radius = (width.clamp(0.2, 6.0) * 0.5) as f64;
        let rgb = self.config.slime_color;
        let rgba = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255];
        let source = self.source(2, id as i64);
        for _ in 0..count {
            let r = &mut self.rng;
            let dir = r.f32() * std::f32::consts::TAU;
            let d = radius * (0.5 + 0.5 * r.f64());
            let speed = r.range(0.03, 0.07);
            let seed = r.next_u32();
            let life = r.range(5.0, 11.0);
            let q = quad * r.range(0.85, 1.15);
            self.push(SpawnRequest {
                family: Family::Slime,
                anim,
                pos: [
                    feet[0] + dir.cos() as f64 * d,
                    feet[1] + 0.05,
                    feet[2] + dir.sin() as f64 * d,
                ],
                vel: [
                    dir.cos() * speed,
                    0.1 + 0.04 * (q / 0.42),
                    dir.sin() * speed,
                ],
                lifetime: life,
                size: [q, q * 0.9],
                rgba,
                rot: 0.0,
                spin: 0.0,
                gravity: 0.05,
                drag: 0.97,
                flutter: 0.0,
                lit: true,
                source,
                seed,
            });
        }
        true
    }

    fn charge(&mut self, e: &EntityObs<'_>) {
        let s = self.scale(Family::Charge);
        let odds = if self.config.reduced_motion {
            40.0
        } else {
            20.0
        };
        if s <= 0.0 || self.rng.f32() * odds >= s || !self.near(e.pos) {
            return;
        }
        let r = &mut self.rng;
        let h = e.height.clamp(0.5, 3.0) as f64;
        let anim = [Anim::ChargeA, Anim::ChargeB, Anim::ChargeC][r.below(3) as usize];
        let size = r.range(0.32, 0.5);
        let pos = [
            e.pos[0] + (r.f64() * 2.0 - 1.0) * 0.45,
            e.pos[1] + h * (0.15 + 0.8 * r.f64()),
            e.pos[2] + (r.f64() * 2.0 - 1.0) * 0.45,
        ];
        let rot = r.f32() * std::f32::consts::TAU;
        let life = r.range(8.0, 11.0);
        let seed = r.next_u32();
        let source = self.source(3, e.id as i64);
        self.push(SpawnRequest {
            family: Family::Charge,
            anim,
            pos,
            vel: [0.0; 3],
            lifetime: life,
            size: [size, size],
            rgba: [
                255,
                255,
                255,
                if self.config.reduced_motion { 170 } else { 255 },
            ],
            rot,
            spin: 0.0,
            gravity: 0.0,
            drag: 1.0,
            flutter: 0.0,
            lit: false,
            source,
            seed,
        });
    }

    fn armor(&mut self, e: &EntityObs<'_>, in_tag: &mut dyn FnMut(&str, &str) -> bool) {
        if e.baby && e.kind.ends_with("zombie") {
            return;
        }
        // Heights for a 1.8 tall player, scaled to the actor.
        let scale = (e.height / 1.8).clamp(0.2, 3.0) as f64;
        let mut slots: [(f64, u32); 4] = [(0.0, 0); 4];
        let mut n = 0;
        for (slot, item) in e.armor.iter().enumerate() {
            let Some(item) = item else { continue };
            if !self.shiny(item, |t, i| in_tag(t, i)) {
                continue;
            }
            let y = match slot {
                0 if e.crouching => 1.55,
                0 => 1.85,
                1 if e.crouching => 1.0,
                1 => 1.1,
                2 => 0.65,
                _ => 0.25,
            };
            slots[n] = (y * scale, material_rgb(item));
            n += 1;
        }
        if n == 0 {
            return;
        }
        let s = self.scale(Family::Armor);
        let odds = (20 - 2 * n as i32) as f32;
        if s <= 0.0 || self.rng.f32() * odds >= s || !self.near(e.pos) {
            return;
        }
        let r = &mut self.rng;
        let (y, rgb) = slots[r.below(n as u32) as usize];
        let w = (e.width.clamp(0.2, 3.0) as f64) * 0.9;
        let pos = [
            e.pos[0] + (r.f64() - 0.5) * w,
            e.pos[1] + y + (r.f64() * 2.0 - 1.0) * 0.12 * scale,
            e.pos[2] + (r.f64() - 0.5) * w,
        ];
        let size = r.range(0.2, 0.3);
        let life = r.range(4.0, 8.0);
        let seed = r.next_u32();
        let source = self.source(4, e.id as i64);
        self.push(SpawnRequest {
            family: Family::Armor,
            anim: Anim::Sparkle,
            pos,
            vel: [0.0; 3],
            lifetime: life,
            size: [size, size],
            rgba: [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255],
            rot: 0.0,
            spin: 0.0,
            gravity: 0.0,
            drag: 1.0,
            flutter: 0.0,
            lit: false,
            source,
            seed,
        });
    }

    fn soul_sprint(&mut self, e: &EntityObs<'_>) {
        let s = self.scale(Family::Soul);
        if s <= 0.0 || self.rng.f32() * 5.0 >= s || !self.near(e.pos) {
            return;
        }
        let r = &mut self.rng;
        let size = r.range(0.28, 0.36);
        let pos = [
            e.pos[0] + (r.f64() - 0.5) * 0.3,
            e.pos[1] + 0.1,
            e.pos[2] + (r.f64() - 0.5) * 0.3,
        ];
        let life = r.range(4.0, 9.0);
        let seed = r.next_u32();
        let source = self.source(5, e.id as i64);
        self.push(soul_request(
            Anim::SoulSprint,
            pos,
            size,
            life,
            0.03,
            seed,
            source,
        ));
    }

    /// One block the game's ambient pass visited. `exposed` has a bit per
    /// face (down, up, north, south, west, east) whose neighbour does not
    /// fully hide it; `above_air` that the block above is air.
    pub fn block_sample(&mut self, class: u8, pos: [i32; 3], exposed: u8, above_air: bool) {
        if class == class::NONE || class == class::UNKNOWN || !self.running() {
            return;
        }
        self.counters.block_samples += 1;
        let center = [
            pos[0] as f64 + 0.5,
            pos[1] as f64 + 0.5,
            pos[2] as f64 + 0.5,
        ];
        if class & class::AMETHYST != 0 {
            let s = self.scale(Family::Amethyst);
            if s > 0.0 && self.rng.f32() * 2.0 < s && self.near(center) {
                let r = &mut self.rng;
                let p = [
                    pos[0] as f64 + 0.2 + 0.6 * r.f64(),
                    pos[1] as f64 + 0.2 + 0.6 * r.f64(),
                    pos[2] as f64 + 0.2 + 0.6 * r.f64(),
                ];
                let size = r.range(0.22, 0.32);
                let life = r.range(5.0, 9.0);
                let seed = r.next_u32();
                let source = self.source(6, block_key(pos));
                self.push(glint(
                    Family::Amethyst,
                    Anim::SparkleAmethyst,
                    p,
                    size,
                    life,
                    0xFFFFFF,
                    seed,
                    source,
                ));
            }
        }
        if class & class::ORE != 0 {
            let s = self.scale(Family::Ores);
            let faces = if self.config.ores_exposed_faces_only {
                exposed
            } else {
                0x3F
            };
            if s > 0.0 && faces != 0 && self.near(center) {
                for face in 0..6u8 {
                    if faces & (1 << face) == 0 || self.rng.f32() * 5.0 >= s {
                        continue;
                    }
                    let r = &mut self.rng;
                    let (a, b) = (r.f64(), r.f64());
                    let out = 0.5625;
                    let off = match face {
                        0 => [a, 0.5 - out, b],
                        1 => [a, 0.5 + out, b],
                        2 => [a, b, 0.5 - out],
                        3 => [a, b, 0.5 + out],
                        4 => [0.5 - out, a, b],
                        _ => [0.5 + out, a, b],
                    };
                    let p = [
                        pos[0] as f64 + off[0],
                        pos[1] as f64 + off[1],
                        pos[2] as f64 + off[2],
                    ];
                    let size = r.range(0.2, 0.3);
                    let life = r.range(4.0, 8.0);
                    let seed = r.next_u32();
                    let source = self.source(7, block_key(pos));
                    let rgb = MATERIALS[((class >> 4) as usize).min(MATERIALS.len() - 1)].1;
                    self.push(glint(
                        Family::Ores,
                        Anim::Sparkle,
                        p,
                        size,
                        life,
                        rgb,
                        seed,
                        source,
                    ));
                }
            }
        }
        if class & class::SOUL != 0 && above_air && self.config.soul_ambient {
            let s = self.scale(Family::Soul);
            if s > 0.0 && self.rng.f32() * 200.0 < s && self.near(center) {
                let r = &mut self.rng;
                let p = [
                    pos[0] as f64 + r.f64(),
                    pos[1] as f64 + 1.1,
                    pos[2] as f64 + r.f64(),
                ];
                let size = r.range(0.32, 0.45);
                let life = r.range(12.0, 24.0);
                let seed = r.next_u32();
                let source = self.source(8, block_key(pos));
                self.push(soul_request(
                    Anim::SoulAmbient,
                    p,
                    size,
                    life,
                    0.022,
                    seed,
                    source,
                ));
            }
        }
    }

    /// Rain ripples: up to the configured samples within the real radius
    /// around `center` (the player's block). `column(x, z)` answers a
    /// loaded column's water surface, or None.
    pub fn water(
        &mut self,
        center: [i32; 3],
        mut column: impl FnMut(i32, i32) -> Option<WaterColumn>,
    ) {
        if !self.running() || self.dimension != "minecraft:overworld" || self.rain <= 0.2 {
            return;
        }
        let s = self.scale(Family::Water);
        if s <= 0.0 {
            return;
        }
        let radius = self.config.water_radius;
        let samples = ((self.config.water_samples as f32 * s * self.rain).round() as u32)
            .min(crate::config::MAX_WATER_SAMPLES);
        let diameter = (2 * radius + 1) as u32;
        let source = self.source(9, 0);
        for _ in 0..samples {
            self.counters.water_samples += 1;
            let x = center[0] + self.rng.below(diameter) as i32 - radius;
            let z = center[2] + self.rng.below(diameter) as i32 - radius;
            let Some(col) = column(x, z) else {
                self.counters.water_rejected += 1;
                continue;
            };
            if (self.config.water_source_only && !col.source)
                || (self.config.water_rain_only && !col.raining)
                || !col.surface_y.is_finite()
            {
                self.counters.water_rejected += 1;
                continue;
            }
            let r = &mut self.rng;
            let p = [
                x as f64 + 0.15 + 0.7 * r.f64(),
                col.surface_y + 0.004,
                z as f64 + 0.15 + 0.7 * r.f64(),
            ];
            let rgb = if self.config.water_biome_tint {
                lighten(col.water_rgb)
            } else {
                0xD8E8F0
            };
            let life = r.range(5.0, 7.0);
            let seed = r.next_u32();
            if !self.near(p) {
                continue;
            }
            self.push(SpawnRequest {
                family: Family::Water,
                anim: Anim::WaterRing,
                pos: p,
                vel: [0.0; 3],
                lifetime: life,
                size: [0.75, 0.75],
                rgba: [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 220],
                rot: 0.0,
                spin: 0.0,
                gravity: 0.0,
                drag: 1.0,
                flutter: 0.0,
                lit: true,
                source,
                seed,
            });
        }
    }

    /// End of the tick: admit requests, advance the pool one step.
    pub fn end_tick(&mut self, light: impl FnMut([i32; 3]) -> u8) {
        if !self.running() {
            self.pool.clear();
            return;
        }
        let l = self.limits();
        self.pool.admit(&l, &mut self.counters);
        self.pool.tick(light, &mut self.counters);
    }

    /// The last frame's draw data and the counters, for the renderer.
    pub fn draw_parts(&mut self) -> (&[Instance], &mut Counters) {
        (&self.instances, &mut self.counters)
    }

    /// Draw data for this frame, empty when nothing should draw.
    pub fn frame(&mut self, partial: f32) -> &[Instance] {
        self.instances.clear();
        if self.running()
            && let Some(atlas) = &self.atlas
        {
            self.pool
                .extract(partial, atlas, self.sky, &mut self.instances);
        }
        &self.instances
    }
}

fn block_key(p: [i32; 3]) -> i64 {
    ((p[0] as i64 & 0x3FF_FFFF) << 38) | ((p[2] as i64 & 0x3FF_FFFF) << 12) | (p[1] as i64 & 0xFFF)
}

/// Brighter, less saturated biome water colour so rings read on water.
fn lighten(rgb: u32) -> u32 {
    let ch = |s: u32| {
        let c = ((rgb >> s) & 0xFF) as f32;
        ((c + (255.0 - c) * 0.55) as u32).min(255) << s
    };
    ch(16) | ch(8) | ch(0)
}

/// Glint tints by material word in an item or block name; the last is the
/// neutral fallback.
const MATERIALS: [(&str, u32); 7] = [
    ("gold", 0xFFE48A),
    ("diamond", 0xB8F6FF),
    ("emerald", 0xA8FFB8),
    ("netherite", 0xE0CFE6),
    ("iron", 0xF2F2F2),
    ("copper", 0xFFC9A8),
    ("", 0xFFFFFF),
];

fn material_index(name: &str) -> u8 {
    MATERIALS
        .iter()
        .position(|(word, _)| name.contains(word))
        .unwrap_or(MATERIALS.len() - 1) as u8
}

/// A glint tint by material name (`golden`, `diamond`, `emerald`...).
pub fn material_rgb(name: &str) -> u32 {
    MATERIALS[material_index(name) as usize].1
}

#[allow(clippy::too_many_arguments)]
fn glint(
    family: Family,
    anim: Anim,
    pos: [f64; 3],
    size: f32,
    life: f32,
    rgb: u32,
    seed: u32,
    source: u64,
) -> SpawnRequest {
    SpawnRequest {
        family,
        anim,
        pos,
        vel: [0.0; 3],
        lifetime: life,
        size: [size, size],
        rgba: [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255],
        rot: 0.0,
        spin: 0.0,
        gravity: 0.0,
        drag: 1.0,
        flutter: 0.0,
        lit: false,
        source,
        seed,
    }
}

fn soul_request(
    anim: Anim,
    pos: [f64; 3],
    size: f32,
    life: f32,
    rise: f32,
    seed: u32,
    source: u64,
) -> SpawnRequest {
    SpawnRequest {
        family: Family::Soul,
        anim,
        pos,
        vel: [0.0, rise, 0.0],
        lifetime: life,
        size: [size, size * 1.1],
        rgba: [255; 4],
        rot: 0.0,
        spin: 0.0,
        gravity: -0.0005,
        drag: 0.98,
        flutter: 0.004,
        lit: false,
        source,
        seed,
    }
}

#[cfg(test)]
mod tests;
