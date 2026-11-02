use super::*;
use crate::config::Config;

fn info<'a>() -> TickInfo<'a> {
    TickInfo {
        native_on: true,
        in_world: true,
        paused: false,
        dimension: "minecraft:overworld",
        camera: [0.0, 64.0, 0.0],
        first_person: true,
        particle_level: 0,
        rain: 0.0,
        sky: 1.0,
        player_underwater: false,
    }
}

fn mob(id: i32, kind: &str) -> EntityObs<'_> {
    EntityObs {
        id,
        kind,
        pos: [1.0, 64.0, 1.0],
        width: 0.6,
        height: 1.99,
        alive: true,
        on_ground: true,
        ..Default::default()
    }
}

fn no_tags(_: &str, _: &str) -> bool {
    false
}

/// One full tick with these entities; returns the particles admitted.
fn tick(e: &mut Engine, inf: &TickInfo<'_>, mobs: &[EntityObs<'_>]) -> u64 {
    let before = e.counters.spawned;
    e.begin_tick(inf);
    for m in mobs {
        e.entity(m, &mut no_tags);
    }
    e.end_entities();
    e.end_tick(|_| 0xF0);
    e.counters.spawned - before
}

fn engine() -> Engine {
    let mut e = Engine::new(Config::default());
    e.begin_tick(&info());
    e
}

fn anims(e: &Engine) -> Vec<Anim> {
    e.pool().live.iter().map(|p| p.anim).collect()
}

#[test]
fn hit_01_02_03_05_mappings_pick_the_right_debris() {
    for (kind, want) in [
        ("minecraft:skeleton", [Anim::BoneA, Anim::BoneB]),
        ("minecraft:stray", [Anim::BoneA, Anim::BoneB]),
        ("minecraft:skeleton_horse", [Anim::BoneA, Anim::BoneB]),
        (
            "minecraft:wither_skeleton",
            [Anim::WitherBoneA, Anim::WitherBoneB],
        ),
        ("minecraft:villager", [Anim::Emerald, Anim::Emerald]),
    ] {
        let mut e = engine();
        e.hurt_signal(5, true, true);
        let n = tick(&mut e, &info(), &[mob(5, kind)]);
        assert!((1..=10).contains(&n), "{kind}: {n}");
        assert!(
            anims(&e).iter().all(|a| want.contains(a)),
            "{kind}: {:?}",
            anims(&e)
        );
    }
}

#[test]
fn hit_04_chicken_variants_use_their_feathers() {
    for (variant, want) in [
        (0, Anim::Feather),
        (1, Anim::FeatherWarm),
        (2, Anim::FeatherCold),
    ] {
        let mut e = engine();
        e.hurt_signal(9, true, true);
        let mut m = mob(9, "minecraft:chicken");
        m.variant = variant;
        m.height = 0.7;
        tick(&mut e, &info(), &[m]);
        let a = anims(&e);
        assert!(
            !a.is_empty() && a.iter().all(|x| *x == want),
            "{variant}: {a:?}"
        );
        // Spawned within the chicken's own height, not a humanoid's.
        assert!(e.pool().live.iter().all(|p| p.pos[1] < 64.0 + 0.7 + 0.2));
    }
}

#[test]
fn hit_06_no_damage_event_no_debris_and_unmapped_ignored() {
    let mut e = engine();
    assert_eq!(tick(&mut e, &info(), &[mob(1, "minecraft:skeleton")]), 0);
    e.hurt_signal(2, true, true);
    assert_eq!(tick(&mut e, &info(), &[mob(2, "minecraft:zombie")]), 0);
    assert_eq!(e.counters.hurt_unmapped, 1);
    // Damage without a source entity (fall, fire): no hit debris.
    e.hurt_signal(3, true, false);
    assert_eq!(tick(&mut e, &info(), &[mob(3, "minecraft:skeleton")]), 0);
    // The hurt animation packet alone is not the canonical hit.
    e.hurt_signal(4, false, true);
    assert_eq!(tick(&mut e, &info(), &[mob(4, "minecraft:skeleton")]), 0);
    assert_eq!(e.counters.hurt_noncanonical, 1);
}

#[test]
fn hit_07_signals_inside_the_window_are_throttled() {
    let mut e = engine();
    let m = [mob(5, "minecraft:skeleton")];
    e.hurt_signal(5, true, true);
    assert!(tick(&mut e, &info(), &m) > 0);
    for _ in 0..9 {
        e.hurt_signal(5, true, true);
        assert_eq!(tick(&mut e, &info(), &m), 0);
    }
    assert_eq!(e.counters.hurt_throttled, 9);
    e.hurt_signal(5, true, true);
    assert!(
        tick(&mut e, &info(), &m) > 0,
        "after the window a new burst comes"
    );
}

#[test]
fn hit_08_two_targets_in_one_tick_are_independent() {
    let mut e = engine();
    e.hurt_signal(1, true, true);
    e.hurt_signal(2, true, true);
    let mut b = mob(2, "minecraft:villager");
    b.pos = [3.0, 64.0, 3.0];
    tick(&mut e, &info(), &[mob(1, "minecraft:skeleton"), b]);
    let a = anims(&e);
    assert!(a.contains(&Anim::Emerald));
    assert!(a.iter().any(|x| matches!(x, Anim::BoneA | Anim::BoneB)));
}

#[test]
fn hit_09_reused_id_and_new_session_inherit_no_cooldown() {
    let mut e = engine();
    let m = [mob(5, "minecraft:skeleton")];
    e.hurt_signal(5, true, true);
    tick(&mut e, &info(), &m);
    e.entity_changed(5); // removed, then a new entity takes the id
    e.hurt_signal(5, true, true);
    assert!(tick(&mut e, &info(), &m) > 0);
    e.new_session();
    e.hurt_signal(5, true, true);
    assert!(tick(&mut e, &info(), &m) > 0);
    // A hurt for an entity that isn't visible is dropped, not kept.
    e.hurt_signal(77, true, true);
    tick(&mut e, &info(), &[]);
    e.entity_changed(5);
    assert_eq!(tick(&mut e, &info(), &[mob(77, "minecraft:skeleton")]), 0);
}

#[test]
fn slm_01_sizes_map_to_three_blob_kinds() {
    for (size, width, want) in [
        (1, 0.52, Anim::SlimeSmall),
        (2, 1.04, Anim::SlimeMedium),
        (4, 2.08, Anim::SlimeLarge),
        (0, 0.2, Anim::SlimeSmall),
        (i32::MAX, 1000.0, Anim::SlimeLarge),
    ] {
        let mut e = engine();
        e.begin_tick(&info());
        assert!(e.slime_landed(3, [1.0, 64.0, 1.0], size, width));
        e.end_tick(|_| 0xF0);
        let a = anims(&e);
        assert!(
            !a.is_empty() && a.iter().all(|x| *x == want),
            "{size}: {a:?}"
        );
        let rgb = e.pool().live[0].rgba;
        assert_eq!(&rgb[..3], &[0x88, 0xFF, 0x79]);
    }
}

#[test]
fn slm_04_disabled_family_keeps_the_games_own_splash() {
    let mut c = Config::default();
    c.family_mut(Family::Slime).enabled = false;
    let mut e = Engine::new(c);
    e.begin_tick(&info());
    assert!(!e.slime_landed(3, [1.0, 64.0, 1.0], 2, 1.04));
    let mut e = Engine::new(Config::default());
    e.begin_tick(&TickInfo {
        native_on: false,
        ..info()
    });
    assert!(!e.slime_landed(3, [1.0, 64.0, 1.0], 2, 1.04));
}

#[test]
fn chg_01_02_only_live_powered_creepers_spark() {
    let mut c = Config::default();
    c.family_mut(Family::Charge).intensity = 2.0;
    let mut e = Engine::new(c);
    let mut powered = mob(1, "minecraft:creeper");
    powered.charged = true;
    let mut total = 0;
    for _ in 0..400 {
        total += tick(&mut e, &info(), &[powered]);
    }
    // About one in ten ticks at double intensity.
    assert!((15..=80).contains(&total), "{total}");
    let mut e = Engine::new(Config::default());
    let plain = mob(2, "minecraft:creeper");
    let mut dead = powered;
    dead.alive = false;
    let mut not_creeper = mob(3, "minecraft:zombie");
    not_creeper.charged = true;
    for _ in 0..400 {
        assert_eq!(tick(&mut e, &info(), &[plain, dead, not_creeper]), 0);
    }
}

#[test]
fn arm_01_03_04_slots_regions_and_self_suppression() {
    let mut c = Config::default();
    c.family_mut(Family::Armor).intensity = 2.0;
    let mut e = Engine::new(c.clone());
    let mut p = mob(1, "minecraft:zombie");
    p.armor[3] = Some("minecraft:golden_boots");
    for _ in 0..200 {
        tick(&mut e, &info(), &[p]);
    }
    assert!(e.counters.family_spawned[Family::Armor.index()] > 0);
    assert!(e.pool().live.iter().all(|x| x.pos[1] < 64.0 + 0.25 + 0.4));
    // Iron is not configured.
    let mut e = Engine::new(c.clone());
    let mut q = mob(1, "minecraft:zombie");
    q.armor[0] = Some("minecraft:iron_helmet");
    for _ in 0..200 {
        assert_eq!(tick(&mut e, &info(), &[q]), 0);
    }
    // Own armor: none in first person, shown in third person.
    let mut me = p;
    me.local = true;
    let mut e = Engine::new(c.clone());
    for _ in 0..200 {
        assert_eq!(tick(&mut e, &info(), &[me]), 0);
    }
    let third = TickInfo {
        first_person: false,
        ..info()
    };
    for _ in 0..200 {
        tick(&mut e, &third, &[me]);
    }
    assert!(e.counters.family_spawned[Family::Armor.index()] > 0);
    // Baby zombies are left out, as in the reference.
    let mut baby = p;
    baby.baby = true;
    let mut e = Engine::new(c);
    for _ in 0..200 {
        assert_eq!(tick(&mut e, &info(), &[baby]), 0);
    }
}

#[test]
fn armor_tags_resolve_through_the_callback() {
    let c = Config {
        armor_items: vec!["#minecraft:trimmable_armor".into()],
        ..Config::default()
    };
    let mut e = Engine::new(c);
    let mut calls = 0;
    assert!(e.shiny("minecraft:iron_boots", |t, i| {
        calls += 1;
        t == "minecraft:trimmable_armor" && i.ends_with("_boots")
    }));
    assert!(e.shiny("minecraft:iron_boots", |_, _| unreachable!("cached")));
    assert_eq!(calls, 1);
}

#[test]
fn block_classes_follow_names_and_tags() {
    let mut e = engine();
    e.resolve_blocks(|tag| match tag {
        "minecraft:gold_ores" => Some(vec![
            "minecraft:gold_ore".into(),
            "minecraft:deepslate_gold_ore".into(),
        ]),
        "minecraft:wither_summon_base_blocks" => Some(vec!["minecraft:soul_sand".into()]),
        _ => None,
    });
    let c = e.block_class(10, || "minecraft:gold_ore".into());
    assert_eq!(c & 7, class::ORE);
    assert_eq!(e.block_class(10, || unreachable!("cached")), c);
    assert_eq!(
        e.block_class(11, || "minecraft:amethyst_cluster".into()),
        class::AMETHYST
    );
    assert_eq!(
        e.block_class(12, || "minecraft:large_amethyst_bud".into()),
        class::NONE
    );
    assert_eq!(
        e.block_class(13, || "minecraft:soul_sand".into()),
        class::SOUL
    );
    assert_eq!(e.block_class(14, || "minecraft:stone".into()), class::NONE);
    assert_eq!(e.block_class(u32::MAX, || unreachable!()), class::NONE);
}

#[test]
fn ame_01_02_full_clusters_sparkle_buds_dont() {
    let mut c = Config::default();
    c.family_mut(Family::Amethyst).intensity = 2.0;
    let mut e = Engine::new(c);
    e.begin_tick(&info());
    let cluster = e.block_class(1, || "minecraft:amethyst_cluster".into());
    let bud = e.block_class(2, || "minecraft:medium_amethyst_bud".into());
    for _ in 0..50 {
        e.block_sample(bud, [2, 64, 2], 0x3F, true);
    }
    e.end_tick(|_| 0xF0);
    assert_eq!(e.active_len(), 0);
    for _ in 0..50 {
        e.begin_tick(&info());
        e.block_sample(cluster, [2, 64, 2], 0x3F, true);
        e.end_tick(|_| 0xF0);
    }
    assert!(e.counters.family_spawned[Family::Amethyst.index()] > 10);
    for p in &e.pool().live {
        assert!((2.0..3.0).contains(&p.pos[0]) && (64.0..65.0).contains(&p.pos[1]));
    }
}

#[test]
fn ore_01_02_buried_ore_never_glints_and_glints_sit_on_open_faces() {
    let mut c = Config::default();
    c.family_mut(Family::Ores).intensity = 2.0;
    let mut e = Engine::new(c);
    e.resolve_blocks(|t| {
        (t == "minecraft:diamond_ores").then(|| vec!["minecraft:diamond_ore".into()])
    });
    e.begin_tick(&info());
    let ore = e.block_class(5, || "minecraft:diamond_ore".into());
    for _ in 0..200 {
        e.begin_tick(&info());
        e.block_sample(ore, [0, 60, 0], 0, false);
        e.end_tick(|_| 0xF0);
    }
    assert_eq!(e.counters.family_spawned[Family::Ores.index()], 0);
    for _ in 0..200 {
        e.begin_tick(&info());
        e.block_sample(ore, [0, 60, 0], 1 << 1, false); // only the top is open
        e.end_tick(|_| 0xF0);
    }
    assert!(e.counters.family_spawned[Family::Ores.index()] > 0);
    for p in &e.pool().live {
        assert!(
            p.pos[1] > 61.0,
            "glint must sit above the open top: {:?}",
            p.pos
        );
        assert_eq!(p.rgba[..3], [0xB8, 0xF6, 0xFF]);
    }
}

#[test]
fn soul_01_02_03_ambient_needs_air_sprint_needs_soul_ground() {
    let mut c = Config::default();
    c.family_mut(Family::Soul).intensity = 2.0;
    let mut e = Engine::new(c.clone());
    e.resolve_blocks(|_| None);
    e.begin_tick(&info());
    let soul = e.block_class(3, || "minecraft:soul_soil".into());
    for _ in 0..3000 {
        e.block_sample(soul, [1, 63, 1], 0x3F, false);
    }
    e.end_tick(|_| 0xF0);
    assert_eq!(e.active_len(), 0, "covered soul blocks stay quiet");
    let mut n = 0;
    for _ in 0..3000 {
        e.begin_tick(&info());
        e.block_sample(soul, [1, 63, 1], 0x3F, true);
        e.end_tick(|_| 0xF0);
        n = n.max(e.counters.family_spawned[Family::Soul.index()]);
    }
    assert!(n > 5, "{n}");
    let mut e = Engine::new(c);
    let mut runner = mob(1, "minecraft:player");
    runner.sprinting = true;
    runner.on_soul_block = false;
    for _ in 0..200 {
        assert_eq!(tick(&mut e, &info(), &[runner]), 0);
    }
    runner.on_soul_block = true;
    let mut standing = runner;
    standing.sprinting = false;
    for _ in 0..200 {
        assert_eq!(tick(&mut e, &info(), &[standing]), 0);
    }
    for _ in 0..200 {
        tick(&mut e, &info(), &[runner]);
    }
    assert!(e.pool().live.iter().any(|p| p.anim == Anim::SoulSprint));
}

fn water_col(source: bool, raining: bool) -> impl FnMut(i32, i32) -> Option<WaterColumn> {
    move |_, _| {
        Some(WaterColumn {
            surface_y: 62.888,
            source,
            raining,
            water_rgb: 0x3F76E4,
        })
    }
}

#[test]
fn wat_01_07_rain_ripples_and_their_negatives() {
    let rainy = TickInfo {
        rain: 1.0,
        ..info()
    };
    let mut e = Engine::new(Config::default());
    e.begin_tick(&rainy);
    e.water([0, 63, 0], water_col(true, true));
    e.end_tick(|_| 0xF0);
    assert!(e.active_len() > 0);
    let p = e.pool().live[0];
    assert_eq!(p.anim, Anim::WaterRing);
    assert!((p.pos[1] - 62.892).abs() < 1e-6);
    assert!(
        p.pos[0].abs() <= 9.0 && p.pos[2].abs() <= 9.0,
        "within the real radius"
    );
    for (inf, col_source, col_rain) in [
        (info(), true, true), // clear weather
        (rainy, false, true), // flowing water
        (rainy, true, false), // roof / snowy biome
        (
            TickInfo {
                dimension: "minecraft:the_nether",
                ..rainy
            },
            true,
            true,
        ), // no weather there
        (
            TickInfo {
                particle_level: 2,
                ..rainy
            },
            true,
            true,
        ), // Minimal
    ] {
        let mut e = Engine::new(Config::default());
        e.begin_tick(&inf);
        e.water([0, 63, 0], water_col(col_source, col_rain));
        e.end_tick(|_| 0xF0);
        assert_eq!(
            e.active_len(),
            0,
            "{:?} {col_source} {col_rain}",
            inf.dimension
        );
    }
    // Unloaded or dry columns are skipped.
    let mut e = Engine::new(Config::default());
    e.begin_tick(&rainy);
    e.water([0, 63, 0], |_, _| None);
    e.end_tick(|_| 0xF0);
    assert_eq!(e.active_len(), 0);
    assert!(e.counters.water_rejected > 0);
}

#[test]
fn life_01_03_master_off_clears_everything_family_off_only_its_own() {
    let mut e = engine();
    e.hurt_signal(1, true, true);
    tick(&mut e, &info(), &[mob(1, "minecraft:skeleton")]);
    assert!(e.slime_landed(2, [0.0, 64.0, 0.0], 2, 1.0));
    e.end_tick(|_| 0xF0);
    assert!(e.pool().family_count(Family::Slime) > 0);
    e.config.family_mut(Family::Slime).enabled = false;
    e.config_changed();
    assert_eq!(e.pool().family_count(Family::Slime), 0);
    assert!(e.pool().family_count(Family::Hit) > 0);
    e.config.enabled = false;
    e.config_changed();
    assert_eq!(e.active_len(), 0);
    assert_eq!(e.state(), Lifecycle::DisabledReady);
    // While off nothing is made.
    e.hurt_signal(1, true, true);
    assert_eq!(tick(&mut e, &info(), &[mob(1, "minecraft:skeleton")]), 0);
}

#[test]
fn life_02_reenabling_replays_nothing() {
    let mut e = engine();
    e.config.enabled = false;
    e.config_changed();
    e.hurt_signal(1, true, true);
    tick(&mut e, &info(), &[]);
    e.config.enabled = true;
    e.config_changed();
    assert_eq!(tick(&mut e, &info(), &[mob(1, "minecraft:skeleton")]), 0);
}

#[test]
fn life_launcher_toggle_off_acts_like_master_off() {
    let mut e = engine();
    e.hurt_signal(1, true, true);
    tick(&mut e, &info(), &[mob(1, "minecraft:skeleton")]);
    assert!(e.active_len() > 0);
    let off = TickInfo {
        native_on: false,
        ..info()
    };
    tick(&mut e, &off, &[]);
    assert_eq!(e.active_len(), 0);
    assert_eq!(e.state(), Lifecycle::DisabledReady);
}

#[test]
fn life_04_pause_freezes_and_resume_has_no_burst() {
    let mut e = engine();
    e.hurt_signal(1, true, true);
    tick(&mut e, &info(), &[mob(1, "minecraft:skeleton")]);
    let n = e.active_len();
    let paused = TickInfo {
        paused: true,
        ..info()
    };
    // The game doesn't tick while paused; even if a frame asks, ages hold.
    let ages: Vec<f32> = e.pool().live.iter().map(|p| p.age).collect();
    e.begin_tick(&paused);
    assert_eq!(
        e.pool().live.iter().map(|p| p.age).collect::<Vec<_>>(),
        ages
    );
    assert_eq!(e.active_len(), n);
}

#[test]
fn life_05_06_leaving_or_changing_dimension_clears_state() {
    let mut e = engine();
    e.hurt_signal(1, true, true);
    tick(&mut e, &info(), &[mob(1, "minecraft:skeleton")]);
    let s = e.session();
    let nether = TickInfo {
        dimension: "minecraft:the_nether",
        ..info()
    };
    tick(&mut e, &nether, &[]);
    assert_eq!(e.active_len(), 0);
    assert!(e.session() > s);
    let out = TickInfo {
        in_world: false,
        ..nether
    };
    e.begin_tick(&out);
    assert_eq!(e.state(), Lifecycle::ReadyNoWorld);
}

#[test]
fn distance_and_minimal_rules() {
    let mut e = engine();
    e.hurt_signal(1, true, true);
    let mut far = mob(1, "minecraft:skeleton");
    far.pos = [100.0, 64.0, 0.0];
    assert_eq!(tick(&mut e, &info(), &[far]), 0);
    assert!(e.counters.rejected_distance > 0);
    let minimal = TickInfo {
        particle_level: 2,
        ..info()
    };
    e.hurt_signal(2, true, true);
    assert_eq!(tick(&mut e, &minimal, &[mob(2, "minecraft:skeleton")]), 0);
    e.config.quality.minimal_mode = MinimalMode::FeedbackOnly;
    e.hurt_signal(3, true, true);
    assert!(tick(&mut e, &minimal, &[mob(3, "minecraft:skeleton")]) > 0);
}

#[test]
fn visual_failure_stops_everything_once() {
    let mut e = engine();
    e.visual_failure("missing atlas".into());
    e.visual_failure("second".into());
    assert_eq!(e.failure.as_deref(), Some("missing atlas"));
    e.hurt_signal(1, true, true);
    assert_eq!(tick(&mut e, &info(), &[mob(1, "minecraft:skeleton")]), 0);
    assert_eq!(e.state(), Lifecycle::VisualFailure);
}

#[test]
fn lim_03_everything_at_once_stays_bounded() {
    let mut c = Config::default();
    for f in Family::ALL {
        c.family_mut(f).intensity = 2.0;
    }
    let mut e = Engine::new(c);
    e.resolve_blocks(|_| Some(vec!["minecraft:gold_ore".into()]));
    let rainy = TickInfo {
        rain: 1.0,
        ..info()
    };
    let ore = e.block_class(1, || "minecraft:gold_ore".into());
    let mut mobs = Vec::new();
    for i in 0..200 {
        let mut m = mob(i, "minecraft:creeper");
        m.charged = true;
        m.armor = [Some("minecraft:diamond_helmet"); 4];
        mobs.push(m);
    }
    for t in 0..400 {
        e.begin_tick(&rainy);
        for (i, m) in mobs.iter().enumerate() {
            e.hurt_signal(i as i32, true, true);
            e.entity(m, &mut no_tags);
        }
        e.end_entities();
        for i in 0..1334 {
            e.block_sample(ore, [i % 30, 60, i / 30], 0x3F, true);
        }
        e.water([0, 63, 0], water_col(true, true));
        e.end_tick(|_| 0xF0);
        assert!(e.active_len() <= 128, "tick {t}: {}", e.active_len());
        assert!(e.pool().pending_len() == 0);
    }
}
