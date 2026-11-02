//! Versioned, validated settings (`config/visuality_horizon.toml`).
//!
//! Loading never panics: each field is read on its own, an invalid value is
//! reported and replaced by its default, and the rest of the file still
//! applies. Saving writes a temporary file beside the target and renames it.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: i64 = 1;
/// Largest config file read; anything bigger is refused before parsing.
pub const MAX_FILE_BYTES: u64 = 256 * 1024;
/// Hard ceiling on owned live particles, whatever the file says.
pub const HARD_MAX_ACTIVE: u32 = 1024;
pub const HARD_MAX_SPAWNS_PER_TICK: u32 = 256;
pub const HARD_MAX_PENDING: u32 = 1024;
/// Longest list (hit rules, armor items, ore tags) kept.
pub const MAX_LIST: usize = 128;
pub const MIN_DISTANCE: f32 = 4.0;
pub const MAX_DISTANCE: f32 = 64.0;
pub const MAX_INTENSITY: f32 = 2.0;
pub const MAX_WATER_RADIUS: i32 = 32;
pub const MAX_WATER_SAMPLES: u32 = 64;

/// The eight effect families.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Family {
    Hit,
    Slime,
    Charge,
    Amethyst,
    Armor,
    Ores,
    Soul,
    Water,
}

impl Family {
    pub const ALL: [Family; 8] = [
        Family::Hit,
        Family::Slime,
        Family::Charge,
        Family::Amethyst,
        Family::Armor,
        Family::Ores,
        Family::Soul,
        Family::Water,
    ];
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn key(self) -> &'static str {
        match self {
            Family::Hit => "hit",
            Family::Slime => "slime",
            Family::Charge => "charge",
            Family::Amethyst => "amethyst",
            Family::Armor => "armor",
            Family::Ores => "ores",
            Family::Soul => "soul",
            Family::Water => "water",
        }
    }
    /// The label players see.
    pub fn label(self) -> &'static str {
        match self {
            Family::Hit => "Hit fragments",
            Family::Slime => "Slime blobs",
            Family::Charge => "Charged creeper sparks",
            Family::Amethyst => "Amethyst sparkles",
            Family::Armor => "Armor glints",
            Family::Ores => "Ore glints",
            Family::Soul => "Soul wisps",
            Family::Water => "Rain ripples",
        }
    }
    /// Feedback effects may replace ambient ones when the budget is full.
    pub fn priority(self) -> Priority {
        match self {
            Family::Hit | Family::Slime => Priority::Feedback,
            Family::Charge | Family::Water => Priority::Contextual,
            Family::Amethyst | Family::Armor | Family::Ores | Family::Soul => Priority::Ambient,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Ambient = 0,
    Contextual = 1,
    Feedback = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Low,
    Balanced,
    High,
    Custom,
}

impl Preset {
    pub fn key(self) -> &'static str {
        match self {
            Preset::Low => "low",
            Preset::Balanced => "balanced",
            Preset::High => "high",
            Preset::Custom => "custom",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Preset::Low => "Low",
            Preset::Balanced => "Balanced",
            Preset::High => "High",
            Preset::Custom => "Custom",
        }
    }
    fn parse(s: &str) -> Option<Preset> {
        Some(match s {
            "low" => Preset::Low,
            "balanced" => Preset::Balanced,
            "high" => Preset::High,
            "custom" => Preset::Custom,
            _ => return None,
        })
    }
}

/// What the game's Minimal particle option does to this mod.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MinimalMode {
    /// Minimal: no extra particles at all.
    NoExtraParticles,
    /// Minimal: feedback effects only (hits, slimes).
    FeedbackOnly,
}

impl MinimalMode {
    fn key(self) -> &'static str {
        match self {
            MinimalMode::NoExtraParticles => "no_extra_particles",
            MinimalMode::FeedbackOnly => "feedback_only",
        }
    }
}

/// The debris an entity drops when hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HitEffect {
    Bone,
    WitherBone,
    /// Feather in the chicken's variant palette.
    FeatherByVariant,
    Feather,
    WarmFeather,
    ColdFeather,
    Emerald,
}

impl HitEffect {
    pub fn key(self) -> &'static str {
        match self {
            HitEffect::Bone => "bone",
            HitEffect::WitherBone => "wither_bone",
            HitEffect::FeatherByVariant => "feather_by_variant",
            HitEffect::Feather => "feather",
            HitEffect::WarmFeather => "warm_feather",
            HitEffect::ColdFeather => "cold_feather",
            HitEffect::Emerald => "emerald",
        }
    }
    pub fn parse(s: &str) -> Option<HitEffect> {
        Some(match s {
            "bone" => HitEffect::Bone,
            "wither_bone" => HitEffect::WitherBone,
            "feather_by_variant" => HitEffect::FeatherByVariant,
            "feather" => HitEffect::Feather,
            "warm_feather" => HitEffect::WarmFeather,
            "cold_feather" => HitEffect::ColdFeather,
            "emerald" => HitEffect::Emerald,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HitRule {
    /// A namespaced entity id (`minecraft:skeleton`) or entity tag (`#minecraft:skeletons`).
    pub entity: String,
    pub effect: HitEffect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FamilyCommon {
    pub enabled: bool,
    pub intensity: f32,
    pub max_active: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Quality {
    pub respect_game_particle_setting: bool,
    pub minimal_mode: MinimalMode,
    pub global_intensity: f32,
    pub emission_distance_blocks: f32,
    pub max_active_particles: u32,
    pub max_spawns_per_tick: u32,
    pub max_pending_requests: u32,
    pub adaptive_density: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub enabled: bool,
    pub preset: Preset,
    pub reduced_motion: bool,
    pub quality: Quality,
    pub families: [FamilyCommon; 8],
    pub hit_cooldown_ticks: u32,
    pub hit_max_fragments: u32,
    pub hit_rules: Vec<HitRule>,
    /// 0xRRGGBB.
    pub slime_color: u32,
    pub amethyst_full_clusters_only: bool,
    pub armor_show_local_first_person: bool,
    pub armor_items: Vec<String>,
    pub ores_exposed_faces_only: bool,
    pub ore_tags: Vec<String>,
    pub soul_ambient: bool,
    pub soul_sprinting: bool,
    pub water_radius: i32,
    pub water_samples: u32,
    pub water_biome_tint: bool,
    pub water_source_only: bool,
    pub water_rain_only: bool,
    pub diagnostics: bool,
    pub show_counters: bool,
    pub collect_frame_trace: bool,
}

/// One problem found while loading, for the log and the settings page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    pub path: String,
    pub message: String,
}

#[derive(Debug)]
pub struct Loaded {
    pub config: Config,
    pub issues: Vec<Issue>,
    /// The file may not be overwritten (future schema, unreadable file).
    pub read_only: bool,
}

fn family_defaults(f: Family) -> FamilyCommon {
    let max_active = match f {
        Family::Hit => 48,
        Family::Slime | Family::Water | Family::Soul => 24,
        Family::Charge => 12,
        Family::Amethyst | Family::Armor | Family::Ores => 16,
    };
    FamilyCommon {
        enabled: true,
        intensity: 1.0,
        max_active,
    }
}

/// The three presets' budget values: (max_active, spawns/tick, distance,
/// water radius, water samples, family scale).
pub fn preset_values(p: Preset) -> Option<(u32, u32, f32, i32, u32, f32)> {
    match p {
        Preset::Low => Some((128, 24, 24.0, 8, 10, 1.0)),
        Preset::Balanced => Some((256, 48, 32.0, 12, 16, 2.0)),
        Preset::High => Some((512, 80, 40.0, 16, 24, 4.0)),
        Preset::Custom => None,
    }
}

pub const DEFAULT_ARMOR: [&str; 8] = [
    "minecraft:golden_helmet",
    "minecraft:golden_chestplate",
    "minecraft:golden_leggings",
    "minecraft:golden_boots",
    "minecraft:diamond_helmet",
    "minecraft:diamond_chestplate",
    "minecraft:diamond_leggings",
    "minecraft:diamond_boots",
];
pub const DEFAULT_ORE_TAGS: [&str; 3] = [
    "minecraft:gold_ores",
    "minecraft:diamond_ores",
    "minecraft:emerald_ores",
];

impl Default for Config {
    fn default() -> Self {
        let mut c = Config {
            enabled: true,
            preset: Preset::Low,
            reduced_motion: false,
            quality: Quality {
                respect_game_particle_setting: true,
                minimal_mode: MinimalMode::NoExtraParticles,
                global_intensity: 1.0,
                emission_distance_blocks: 24.0,
                max_active_particles: 128,
                max_spawns_per_tick: 24,
                max_pending_requests: 256,
                adaptive_density: false,
            },
            families: Family::ALL.map(family_defaults),
            hit_cooldown_ticks: 10,
            hit_max_fragments: 10,
            hit_rules: [
                ("minecraft:skeleton", HitEffect::Bone),
                ("minecraft:skeleton_horse", HitEffect::Bone),
                ("minecraft:stray", HitEffect::Bone),
                ("minecraft:wither_skeleton", HitEffect::WitherBone),
                ("minecraft:chicken", HitEffect::FeatherByVariant),
                ("minecraft:villager", HitEffect::Emerald),
            ]
            .into_iter()
            .map(|(entity, effect)| HitRule {
                entity: entity.into(),
                effect,
            })
            .collect(),
            slime_color: 0x88FF79,
            amethyst_full_clusters_only: true,
            armor_show_local_first_person: false,
            armor_items: DEFAULT_ARMOR.iter().map(|s| s.to_string()).collect(),
            ores_exposed_faces_only: true,
            ore_tags: DEFAULT_ORE_TAGS.iter().map(|s| s.to_string()).collect(),
            soul_ambient: true,
            soul_sprinting: true,
            water_radius: 8,
            water_samples: 10,
            water_biome_tint: true,
            water_source_only: true,
            water_rain_only: true,
            diagnostics: false,
            show_counters: false,
            collect_frame_trace: false,
        };
        c.apply_preset(Preset::Low);
        c
    }
}

impl Config {
    pub fn family(&self, f: Family) -> &FamilyCommon {
        &self.families[f.index()]
    }
    pub fn family_mut(&mut self, f: Family) -> &mut FamilyCommon {
        &mut self.families[f.index()]
    }

    /// Sets every preset-controlled value. Custom changes nothing.
    pub fn apply_preset(&mut self, p: Preset) {
        let Some((active, spawns, distance, radius, samples, scale)) = preset_values(p) else {
            self.preset = Preset::Custom;
            return;
        };
        self.preset = p;
        self.quality.max_active_particles = active;
        self.quality.max_spawns_per_tick = spawns;
        self.quality.emission_distance_blocks = distance;
        self.water_radius = radius;
        self.water_samples = samples;
        for f in Family::ALL {
            let base = family_defaults(f).max_active as f32;
            self.family_mut(f).max_active = ((base * scale) as u32).min(HARD_MAX_ACTIVE);
        }
    }

    /// The preset the preset-controlled values match, else Custom.
    pub fn detect_preset(&self) -> Preset {
        for p in [Preset::Low, Preset::Balanced, Preset::High] {
            let mut probe = self.clone();
            probe.apply_preset(p);
            if probe.quality == self.quality
                && probe.water_radius == self.water_radius
                && probe.water_samples == self.water_samples
                && probe.families == self.families
            {
                return p;
            }
        }
        Preset::Custom
    }

    /// Clamps every number into its documented range (defence in depth for
    /// values changed in code; loading already validates).
    pub fn clamp(&mut self) {
        let q = &mut self.quality;
        q.global_intensity = clamp_f(q.global_intensity, 0.0, MAX_INTENSITY, 1.0);
        q.emission_distance_blocks =
            clamp_f(q.emission_distance_blocks, MIN_DISTANCE, MAX_DISTANCE, 24.0);
        q.max_active_particles = q.max_active_particles.clamp(1, HARD_MAX_ACTIVE);
        q.max_spawns_per_tick = q.max_spawns_per_tick.clamp(1, HARD_MAX_SPAWNS_PER_TICK);
        q.max_pending_requests = q.max_pending_requests.clamp(1, HARD_MAX_PENDING);
        for f in &mut self.families {
            f.intensity = clamp_f(f.intensity, 0.0, MAX_INTENSITY, 1.0);
            f.max_active = f.max_active.min(HARD_MAX_ACTIVE);
        }
        self.hit_cooldown_ticks = self.hit_cooldown_ticks.min(200);
        self.hit_max_fragments = self.hit_max_fragments.clamp(1, 10);
        self.slime_color &= 0xFF_FFFF;
        self.water_radius = self.water_radius.clamp(1, MAX_WATER_RADIUS);
        self.water_samples = self.water_samples.clamp(1, MAX_WATER_SAMPLES);
        self.hit_rules.truncate(MAX_LIST);
        self.armor_items.truncate(MAX_LIST);
        self.ore_tags.truncate(MAX_LIST);
    }

    /// The file's text: the documented schema, every field written.
    pub fn to_toml(&self) -> String {
        let mut s = String::new();
        let q = &self.quality;
        let _ = writeln!(
            s,
            "# Visuality for Horizon settings. Edit in game: Options > Mods > Visuality.\n\
             # Values outside their range are reported and replaced by defaults.\n\
             schema_version = {SCHEMA_VERSION}\nenabled = {}\npreset = \"{}\"\nreduced_motion = {}\n",
            self.enabled,
            self.preset.key(),
            self.reduced_motion
        );
        let _ = writeln!(
            s,
            "[quality]\nrespect_game_particle_setting = {}\nminimal_mode = \"{}\"\n\
             global_intensity = {}\nemission_distance_blocks = {}\nmax_active_particles = {}\n\
             max_spawns_per_tick = {}\nmax_pending_requests = {}\nadaptive_density = {}\n",
            q.respect_game_particle_setting,
            q.minimal_mode.key(),
            fmt_f(q.global_intensity),
            fmt_f(q.emission_distance_blocks),
            q.max_active_particles,
            q.max_spawns_per_tick,
            q.max_pending_requests,
            q.adaptive_density
        );
        for f in Family::ALL {
            let c = self.family(f);
            let _ = writeln!(
                s,
                "[effects.{}]\nenabled = {}\nintensity = {}\nmax_active = {}",
                f.key(),
                c.enabled,
                fmt_f(c.intensity),
                c.max_active
            );
            match f {
                Family::Hit => {
                    let _ = writeln!(
                        s,
                        "cooldown_ticks = {}\nmax_fragments_per_event = {}",
                        self.hit_cooldown_ticks, self.hit_max_fragments
                    );
                    for r in &self.hit_rules {
                        let _ = writeln!(
                            s,
                            "\n[[effects.hit.rules]]\nentity = {}\neffect = \"{}\"",
                            quote(&r.entity),
                            r.effect.key()
                        );
                    }
                }
                Family::Slime => {
                    let _ = writeln!(s, "color = \"#{:06X}\"", self.slime_color);
                }
                Family::Charge => {}
                Family::Amethyst => {
                    let _ = writeln!(
                        s,
                        "full_clusters_only = {}",
                        self.amethyst_full_clusters_only
                    );
                }
                Family::Armor => {
                    let _ = writeln!(
                        s,
                        "show_local_player_first_person = {}\nitems = {}",
                        self.armor_show_local_first_person,
                        list(&self.armor_items)
                    );
                }
                Family::Ores => {
                    let _ = writeln!(
                        s,
                        "exposed_faces_only = {}\ntags = {}",
                        self.ores_exposed_faces_only,
                        list(&self.ore_tags)
                    );
                }
                Family::Soul => {
                    let _ = writeln!(
                        s,
                        "ambient_enabled = {}\nsprinting_enabled = {}",
                        self.soul_ambient, self.soul_sprinting
                    );
                }
                Family::Water => {
                    let _ = writeln!(
                        s,
                        "search_radius_blocks = {}\ncandidate_samples_per_tick = {}\n\
                         biome_tint = {}\nsource_water_only = {}\nrain_only = {}",
                        self.water_radius,
                        self.water_samples,
                        self.water_biome_tint,
                        self.water_source_only,
                        self.water_rain_only
                    );
                }
            }
            s.push('\n');
        }
        let _ = writeln!(
            s,
            "[diagnostics]\nenabled = {}\nshow_counters = {}\ncollect_frame_trace = {}",
            self.diagnostics, self.show_counters, self.collect_frame_trace
        );
        s
    }
}

fn fmt_f(v: f32) -> String {
    let s = format!("{v}");
    if s.contains('.') { s } else { format!("{s}.0") }
}
fn quote(s: &str) -> String {
    toml::Value::String(s.to_owned()).to_string()
}
fn list(items: &[String]) -> String {
    if items.is_empty() {
        return "[]".into();
    }
    let body: Vec<String> = items.iter().map(|i| format!("  {},", quote(i))).collect();
    format!("[\n{}\n]", body.join("\n"))
}
fn clamp_f(v: f32, lo: f32, hi: f32, default: f32) -> f32 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        default
    }
}

/// `namespace:path` with Minecraft's allowed characters, optionally `#` for a tag.
pub fn valid_id(s: &str, allow_tag: bool) -> bool {
    let s = if allow_tag {
        s.strip_prefix('#').unwrap_or(s)
    } else {
        s
    };
    let Some((ns, path)) = s.split_once(':') else {
        return false;
    };
    !ns.is_empty()
        && !path.is_empty()
        && s.len() <= 128
        && ns
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-.".contains(&b))
        && path
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-./".contains(&b))
}

/// Strict `#RRGGBB`.
pub fn parse_color(s: &str) -> Option<u32> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

struct Reader<'a> {
    issues: &'a mut Vec<Issue>,
}

impl Reader<'_> {
    fn issue(&mut self, path: &str, message: impl Into<String>) {
        self.issues.push(Issue {
            path: path.into(),
            message: message.into(),
        });
    }
    fn table<'t>(&mut self, t: &'t toml::Table, key: &str, path: &str) -> Option<&'t toml::Table> {
        match t.get(key)? {
            toml::Value::Table(x) => Some(x),
            _ => {
                self.issue(path, "expected a table");
                None
            }
        }
    }
    fn bool(&mut self, t: &toml::Table, key: &str, path: &str, out: &mut bool) {
        match t.get(key) {
            None => {}
            Some(toml::Value::Boolean(b)) => *out = *b,
            Some(_) => self.issue(path, "expected true or false"),
        }
    }
    fn float(&mut self, t: &toml::Table, key: &str, path: &str, lo: f32, hi: f32, out: &mut f32) {
        let v = match t.get(key) {
            None => return,
            Some(toml::Value::Float(f)) => *f,
            Some(toml::Value::Integer(i)) => *i as f64,
            Some(_) => return self.issue(path, "expected a number"),
        };
        if v.is_finite() && (lo as f64..=hi as f64).contains(&v) {
            *out = v as f32;
        } else {
            self.issue(path, format!("must be between {lo} and {hi}"));
        }
    }
    fn int(&mut self, t: &toml::Table, key: &str, path: &str, lo: i64, hi: i64) -> Option<i64> {
        match t.get(key) {
            None => None,
            Some(toml::Value::Integer(i)) if (lo..=hi).contains(i) => Some(*i),
            Some(toml::Value::Integer(_)) => {
                self.issue(path, format!("must be between {lo} and {hi}"));
                None
            }
            Some(_) => {
                self.issue(path, "expected a whole number");
                None
            }
        }
    }
    fn uint(&mut self, t: &toml::Table, key: &str, path: &str, lo: u32, hi: u32, out: &mut u32) {
        if let Some(v) = self.int(t, key, path, lo as i64, hi as i64) {
            *out = v as u32;
        }
    }
    fn ids(&mut self, t: &toml::Table, key: &str, path: &str, tags: bool) -> Option<Vec<String>> {
        let arr = match t.get(key)? {
            toml::Value::Array(a) => a,
            _ => {
                self.issue(path, "expected a list");
                return None;
            }
        };
        if arr.len() > MAX_LIST {
            self.issue(path, format!("more than {MAX_LIST} entries; extra ignored"));
        }
        let mut out = Vec::new();
        for (i, v) in arr.iter().take(MAX_LIST).enumerate() {
            match v.as_str() {
                Some(s) if valid_id(s, tags) => {
                    if !out.iter().any(|o| o == s) {
                        out.push(s.to_owned());
                    }
                }
                _ => self.issue(&format!("{path}[{i}]"), "not a namespaced id"),
            }
        }
        Some(out)
    }
}

/// Parses the file's text. Unknown keys are ignored; bad values are reported.
pub fn parse(text: &str) -> Loaded {
    let mut issues = Vec::new();
    let mut c = Config::default();
    let root: toml::Table = match text.parse() {
        Ok(t) => t,
        Err(e) => {
            let first = e.to_string().lines().next().unwrap_or("").to_owned();
            issues.push(Issue {
                path: String::new(),
                message: format!("not valid TOML ({first}); using defaults"),
            });
            return Loaded {
                config: c,
                issues,
                read_only: true,
            };
        }
    };
    let mut read_only = false;
    match root.get("schema_version") {
        Some(toml::Value::Integer(v)) if *v == SCHEMA_VERSION => {}
        Some(toml::Value::Integer(v)) if *v > SCHEMA_VERSION => {
            issues.push(Issue {
                path: "schema_version".into(),
                message: format!(
                    "written by a newer version ({v}); read what is understood and left unchanged"
                ),
            });
            read_only = true;
        }
        None => {}
        Some(_) => issues.push(Issue {
            path: "schema_version".into(),
            message: "expected 1".into(),
        }),
    }
    let mut r = Reader {
        issues: &mut issues,
    };
    r.bool(&root, "enabled", "enabled", &mut c.enabled);
    r.bool(
        &root,
        "reduced_motion",
        "reduced_motion",
        &mut c.reduced_motion,
    );
    let mut preset = Preset::Low;
    match root.get("preset") {
        None => {}
        Some(v) => match v.as_str().and_then(Preset::parse) {
            Some(p) => preset = p,
            None => r.issue("preset", "expected low, balanced, high or custom"),
        },
    }
    c.apply_preset(if preset == Preset::Custom {
        Preset::Low
    } else {
        preset
    });
    if let Some(q) = r.table(&root, "quality", "quality") {
        let out = &mut c.quality;
        r.bool(
            q,
            "respect_game_particle_setting",
            "quality.respect_game_particle_setting",
            &mut out.respect_game_particle_setting,
        );
        match q.get("minimal_mode").map(|v| v.as_str()) {
            None => {}
            Some(Some("no_extra_particles")) => out.minimal_mode = MinimalMode::NoExtraParticles,
            Some(Some("feedback_only")) => out.minimal_mode = MinimalMode::FeedbackOnly,
            Some(_) => r.issue(
                "quality.minimal_mode",
                "expected no_extra_particles or feedback_only",
            ),
        }
        r.float(
            q,
            "global_intensity",
            "quality.global_intensity",
            0.0,
            MAX_INTENSITY,
            &mut out.global_intensity,
        );
        r.float(
            q,
            "emission_distance_blocks",
            "quality.emission_distance_blocks",
            MIN_DISTANCE,
            MAX_DISTANCE,
            &mut out.emission_distance_blocks,
        );
        r.uint(
            q,
            "max_active_particles",
            "quality.max_active_particles",
            1,
            HARD_MAX_ACTIVE,
            &mut out.max_active_particles,
        );
        r.uint(
            q,
            "max_spawns_per_tick",
            "quality.max_spawns_per_tick",
            1,
            HARD_MAX_SPAWNS_PER_TICK,
            &mut out.max_spawns_per_tick,
        );
        r.uint(
            q,
            "max_pending_requests",
            "quality.max_pending_requests",
            1,
            HARD_MAX_PENDING,
            &mut out.max_pending_requests,
        );
        r.bool(
            q,
            "adaptive_density",
            "quality.adaptive_density",
            &mut out.adaptive_density,
        );
    }
    let effects = r.table(&root, "effects", "effects").cloned();
    if let Some(effects) = &effects {
        for key in effects.keys() {
            if !Family::ALL.iter().any(|f| f.key() == key) {
                r.issue(&format!("effects.{key}"), "unknown effect; ignored");
            }
        }
        for f in Family::ALL {
            let path = format!("effects.{}", f.key());
            let Some(t) = r.table(effects, f.key(), &path) else {
                continue;
            };
            let mut common = c.family(f).clone();
            r.bool(
                t,
                "enabled",
                &format!("{path}.enabled"),
                &mut common.enabled,
            );
            r.float(
                t,
                "intensity",
                &format!("{path}.intensity"),
                0.0,
                MAX_INTENSITY,
                &mut common.intensity,
            );
            r.uint(
                t,
                "max_active",
                &format!("{path}.max_active"),
                0,
                HARD_MAX_ACTIVE,
                &mut common.max_active,
            );
            *c.family_mut(f) = common;
            match f {
                Family::Hit => {
                    r.uint(
                        t,
                        "cooldown_ticks",
                        "effects.hit.cooldown_ticks",
                        0,
                        200,
                        &mut c.hit_cooldown_ticks,
                    );
                    r.uint(
                        t,
                        "max_fragments_per_event",
                        "effects.hit.max_fragments_per_event",
                        1,
                        10,
                        &mut c.hit_max_fragments,
                    );
                    if let Some(v) = t.get("rules") {
                        c.hit_rules = read_rules(&mut r, v);
                    }
                }
                Family::Slime => match t.get("color") {
                    None => {}
                    Some(v) => match v.as_str().and_then(parse_color) {
                        Some(col) => c.slime_color = col,
                        None => r.issue("effects.slime.color", "expected \"#RRGGBB\""),
                    },
                },
                Family::Charge => {}
                Family::Amethyst => r.bool(
                    t,
                    "full_clusters_only",
                    "effects.amethyst.full_clusters_only",
                    &mut c.amethyst_full_clusters_only,
                ),
                Family::Armor => {
                    r.bool(
                        t,
                        "show_local_player_first_person",
                        "effects.armor.show_local_player_first_person",
                        &mut c.armor_show_local_first_person,
                    );
                    if let Some(items) = r.ids(t, "items", "effects.armor.items", true) {
                        c.armor_items = items;
                    }
                }
                Family::Ores => {
                    r.bool(
                        t,
                        "exposed_faces_only",
                        "effects.ores.exposed_faces_only",
                        &mut c.ores_exposed_faces_only,
                    );
                    if let Some(tags) = r.ids(t, "tags", "effects.ores.tags", true) {
                        c.ore_tags = tags;
                    }
                }
                Family::Soul => {
                    r.bool(
                        t,
                        "ambient_enabled",
                        "effects.soul.ambient_enabled",
                        &mut c.soul_ambient,
                    );
                    r.bool(
                        t,
                        "sprinting_enabled",
                        "effects.soul.sprinting_enabled",
                        &mut c.soul_sprinting,
                    );
                }
                Family::Water => {
                    if let Some(v) = r.int(
                        t,
                        "search_radius_blocks",
                        "effects.water.search_radius_blocks",
                        1,
                        MAX_WATER_RADIUS as i64,
                    ) {
                        c.water_radius = v as i32;
                    }
                    r.uint(
                        t,
                        "candidate_samples_per_tick",
                        "effects.water.candidate_samples_per_tick",
                        1,
                        MAX_WATER_SAMPLES,
                        &mut c.water_samples,
                    );
                    r.bool(
                        t,
                        "biome_tint",
                        "effects.water.biome_tint",
                        &mut c.water_biome_tint,
                    );
                    r.bool(
                        t,
                        "source_water_only",
                        "effects.water.source_water_only",
                        &mut c.water_source_only,
                    );
                    r.bool(
                        t,
                        "rain_only",
                        "effects.water.rain_only",
                        &mut c.water_rain_only,
                    );
                }
            }
        }
    }
    if let Some(d) = r.table(&root, "diagnostics", "diagnostics") {
        r.bool(d, "enabled", "diagnostics.enabled", &mut c.diagnostics);
        r.bool(
            d,
            "show_counters",
            "diagnostics.show_counters",
            &mut c.show_counters,
        );
        r.bool(
            d,
            "collect_frame_trace",
            "diagnostics.collect_frame_trace",
            &mut c.collect_frame_trace,
        );
    }
    c.clamp();
    c.preset = c.detect_preset();
    Loaded {
        config: c,
        issues,
        read_only,
    }
}

fn read_rules(r: &mut Reader<'_>, v: &toml::Value) -> Vec<HitRule> {
    let Some(arr) = v.as_array() else {
        r.issue(
            "effects.hit.rules",
            "expected a list of [[effects.hit.rules]]",
        );
        return Config::default().hit_rules;
    };
    if arr.len() > MAX_LIST {
        r.issue(
            "effects.hit.rules",
            format!("more than {MAX_LIST} rules; extra ignored"),
        );
    }
    let mut out: Vec<HitRule> = Vec::new();
    for (i, rule) in arr.iter().take(MAX_LIST).enumerate() {
        let path = format!("effects.hit.rules[{i}]");
        let entity = rule.get("entity").and_then(|e| e.as_str());
        let effect = rule.get("effect").and_then(|e| e.as_str());
        match (entity, effect) {
            (Some(e), Some(eff)) if valid_id(e, false) => match HitEffect::parse(eff) {
                Some(effect) => {
                    out.retain(|o| o.entity != e);
                    out.push(HitRule {
                        entity: e.to_owned(),
                        effect,
                    });
                }
                None => r.issue(&path, format!("unknown effect \"{eff}\"")),
            },
            (Some(e), _) if !valid_id(e, false) => r.issue(
                &path,
                "entity must be a namespaced entity id (entity tags are not available on the client)",
            ),
            _ => r.issue(&path, "needs entity and effect"),
        }
    }
    out
}

/// Reads the file at `path`, never failing: a missing file is the defaults,
/// an oversized or malformed one is kept beside it as `.invalid` for
/// diagnosis.
pub fn load(path: &Path) -> Loaded {
    match std::fs::metadata(path) {
        Err(_) => {
            return Loaded {
                config: Config::default(),
                issues: Vec::new(),
                read_only: false,
            };
        }
        Ok(m) if m.len() > MAX_FILE_BYTES => {
            preserve_invalid(path);
            return Loaded {
                config: Config::default(),
                issues: vec![Issue {
                    path: String::new(),
                    message: format!(
                        "larger than {MAX_FILE_BYTES} bytes; kept as .invalid, using defaults"
                    ),
                }],
                read_only: false,
            };
        }
        Ok(_) => {}
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
            // Not text at all: malformed, kept for diagnosis.
            preserve_invalid(path);
            return Loaded {
                config: Config::default(),
                issues: vec![Issue {
                    path: String::new(),
                    message: "not UTF-8 text; kept as .invalid, using defaults".into(),
                }],
                read_only: false,
            };
        }
        Err(e) => {
            return Loaded {
                config: Config::default(),
                issues: vec![Issue {
                    path: String::new(),
                    message: format!("could not be read ({e}); using defaults"),
                }],
                read_only: true,
            };
        }
    };
    let mut loaded = parse(&text);
    if loaded.read_only && loaded.issues.iter().any(|i| i.path.is_empty()) {
        // Malformed: keep the original for diagnosis, then the defaults may be saved.
        preserve_invalid(path);
        loaded.read_only = false;
    }
    loaded
}

fn invalid_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".invalid");
    path.with_file_name(name)
}

fn preserve_invalid(path: &Path) {
    let _ = std::fs::copy(path, invalid_path(path));
}

/// Writes `config` atomically: a temporary file in the same folder, then a
/// rename over the target. The old file stays whole if anything fails.
pub fn save(path: &Path, config: &Config) -> std::io::Result<()> {
    let text = config.to_toml();
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
    }
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cfg_01_defaults_round_trip() {
        let c = Config::default();
        let back = parse(&c.to_toml());
        assert!(back.issues.is_empty(), "{:?}", back.issues);
        assert_eq!(back.config, c);
        assert_eq!(c.preset, Preset::Low);
        assert_eq!(c.quality.max_active_particles, 128);
        assert_eq!(c.quality.max_spawns_per_tick, 24);
        assert_eq!(c.slime_color, 0x88FF79);
        assert_eq!(c.hit_rules.len(), 6);
    }

    #[test]
    fn cfg_02_every_family_round_trips_changed_values() {
        let mut c = Config {
            enabled: false,
            reduced_motion: true,
            ..Config::default()
        };
        c.family_mut(Family::Ores).enabled = false;
        c.family_mut(Family::Water).intensity = 1.5;
        c.slime_color = 0x123456;
        c.armor_items = vec!["minecraft:netherite_helmet".into()];
        c.hit_rules.push(HitRule {
            entity: "minecraft:bogged".into(),
            effect: HitEffect::Bone,
        });
        c.preset = c.detect_preset();
        let back = parse(&c.to_toml());
        assert!(back.issues.is_empty(), "{:?}", back.issues);
        assert_eq!(back.config, c);
    }

    #[test]
    fn cfg_03_malformed_text_falls_back() {
        let l = parse("enabled = [");
        assert_eq!(l.config, Config::default());
        assert_eq!(l.issues.len(), 1);
    }

    #[test]
    fn cfg_04_bad_values_are_reported_and_others_kept() {
        let l = parse(
            "preset = \"ultra\"\nenabled = \"yes\"\n[effects.water]\nenabled = false\n\
             search_radius_blocks = 9000\n[effects.slime]\ncolor = \"green\"\n\
             [[effects.hit.rules]]\nentity = \"Skeleton\"\neffect = \"bone\"\n\
             [[effects.hit.rules]]\nentity = \"minecraft:zombie\"\neffect = \"glitter\"\n\
             [[effects.hit.rules]]\nentity = \"minecraft:cow\"\neffect = \"emerald\"\n\
             [effects.sparkly]\nenabled = true\n",
        );
        let paths: Vec<&str> = l.issues.iter().map(|i| i.path.as_str()).collect();
        for p in [
            "preset",
            "enabled",
            "effects.water.search_radius_blocks",
            "effects.slime.color",
            "effects.hit.rules[0]",
            "effects.hit.rules[1]",
            "effects.sparkly",
        ] {
            assert!(paths.contains(&p), "{p} missing from {paths:?}");
        }
        assert!(!l.config.family(Family::Water).enabled);
        assert_eq!(l.config.water_radius, 8);
        assert_eq!(l.config.slime_color, 0x88FF79);
        assert_eq!(
            l.config.hit_rules,
            vec![HitRule {
                entity: "minecraft:cow".into(),
                effect: HitEffect::Emerald
            }]
        );
    }

    #[test]
    fn cfg_05_non_finite_and_huge_values_rejected() {
        let l = parse(
            "[quality]\nglobal_intensity = nan\nmax_active_particles = 100000000\n\
             emission_distance_blocks = inf\n",
        );
        assert_eq!(l.config.quality.global_intensity, 1.0);
        assert_eq!(l.config.quality.max_active_particles, 128);
        assert_eq!(l.config.quality.emission_distance_blocks, 24.0);
        assert_eq!(l.issues.len(), 3);
        let many: String = (0..500)
            .map(|i| format!("\"minecraft:item_{i}\","))
            .collect();
        let l = parse(&format!("[effects.armor]\nitems = [{many}]"));
        assert_eq!(l.config.armor_items.len(), MAX_LIST);
    }

    #[test]
    fn cfg_06_future_schema_is_read_only() {
        let l = parse("schema_version = 7\nenabled = false\n");
        assert!(l.read_only);
        assert!(!l.config.enabled);
    }

    #[test]
    fn cfg_09_manual_change_shows_custom() {
        let mut c = Config::default();
        assert_eq!(c.detect_preset(), Preset::Low);
        c.apply_preset(Preset::High);
        assert_eq!(c.detect_preset(), Preset::High);
        c.quality.max_active_particles = 300;
        assert_eq!(c.detect_preset(), Preset::Custom);
    }

    #[test]
    fn cfg_10_reset_restores_shipped_defaults() {
        let shipped = Config::default();
        let mut c = shipped.clone();
        c.apply_preset(Preset::High);
        c.enabled = false;
        c = Config::default();
        assert_eq!(c, shipped);
    }

    #[test]
    fn colors_and_ids_are_strict() {
        assert_eq!(parse_color("#88FF79"), Some(0x88FF79));
        for bad in ["88FF79", "#88FF7", "#88FF79A", "#GGGGGG", ""] {
            assert_eq!(parse_color(bad), None, "{bad}");
        }
        assert!(valid_id("minecraft:gold_ores", false));
        assert!(valid_id("#minecraft:gold_ores", true));
        assert!(!valid_id("#minecraft:gold_ores", false));
        assert!(!valid_id("Minecraft:Gold", false));
        assert!(!valid_id("gold", false));
        assert!(!valid_id("../x:y", false));
    }

    #[test]
    fn cfg_07_08_save_is_atomic_and_load_survives_garbage() {
        let dir = std::env::temp_dir().join(format!("vh-cfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("visuality_horizon.toml");
        let l = load(&path);
        assert_eq!(l.config, Config::default());
        let mut c = Config::default();
        c.family_mut(Family::Soul).enabled = false;
        save(&path, &c).unwrap();
        assert_eq!(load(&path).config, c);
        assert!(!dir.join("visuality_horizon.toml.tmp").exists());
        std::fs::write(&path, b"\x00\xff garbage [[[").unwrap();
        let l = load(&path);
        assert_eq!(l.config, Config::default());
        assert!(dir.join("visuality_horizon.toml.invalid").exists());
        // A save into a folder that is gone fails cleanly.
        let missing = dir.join("missing").join("x.toml");
        assert!(save(&missing, &c).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
