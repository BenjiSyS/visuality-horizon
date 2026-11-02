// The game side of Visuality for Horizon: copies what the window's client
// already knows (visible entities, its own player, weather, loaded blocks)
// into the effect engine, and serves the settings page. It reads the game
// only; nothing here changes the world, the player or what is sent.
// Ordinary comments only: this file is a support module payload.

use horizon_client_render::gfx::native_render::visuality_horizon::engine as vh;
use vh::engine::{EntityObs, TickInfo, WaterColumn, class};

fn config_dir() -> Option<std::path::PathBuf> {
    super::Game::native_config_dir().ok()
}

fn micros(start: std::time::Instant) -> u32 {
    start.elapsed().as_micros().min(u32::MAX as u128) as u32
}

pub(super) fn hurt(id: i32, canonical: bool, attacker: bool) {
    vh::runtime::with_existing(|rt| rt.engine.hurt_signal(id, canonical, attacker));
}

pub(super) fn entity_changed(id: i32) {
    vh::runtime::with_existing(|rt| rt.engine.entity_changed(id));
}

pub(super) fn new_session() {
    vh::runtime::with_existing(|rt| rt.engine.new_session());
}

/// The soul class of the block an entity stands on (`floor(y - 0.2)`, as the
/// game's sprint particles look it up).
fn on_soul(game: &super::Game<'_>, rt: &mut vh::runtime::Runtime, p: [f64; 3]) -> bool {
    let (x, y, z) = (p[0].floor() as i32, (p[1] - 0.2).floor() as i32, p[2].floor() as i32);
    if !game.world.loaded(x, z) {
        return false;
    }
    let state = game.world.block(x, y, z);
    let facts = game.res.facts();
    rt.engine.block_class(state.0, || facts.get(state).name.clone()) & class::SOUL != 0
}

/// Once a client tick, on the window's game only.
pub(super) fn tick(game: &mut super::Game<'_>) {
    if !game.native_window() {
        return;
    }
    let start = std::time::Instant::now();
    let game = &*game;
    vh::runtime::with(config_dir, |rt| {
        let dimension = game.dim.id();
        let sky = (15 - game.sky_darken().clamp(0, 15)) as f32 / 15.0;
        let info = TickInfo {
            native_on: game.native_on(vh::MOD_ID),
            in_world: game.player.is_some(),
            paused: game.pause,
            dimension,
            camera: game.view.position,
            first_person: game.camera_type.first_person(),
            particle_level: game.settings.particles,
            rain: game.native_weather().0,
            sky,
            player_underwater: false,
        };
        rt.engine.begin_tick(&info);
        if rt.engine.state() != vh::engine::Lifecycle::ActiveWorld {
            rt.engine.end_tick(|_| 0xF0);
            return;
        }
        let items = game.items();
        if rt.needs_resolve() {
            let unknown = rt.engine.resolve_blocks(|tag| {
                items
                    .block_tags
                    .get(tag)
                    .map(|names| names.iter().cloned().collect())
            });
            rt.resolved(unknown);
        }
        let mut in_tag = |tag: &str, item: &str| items.item_in_tag(tag, item);

        // Other entities the client shows.
        // Entities too far to emit anything are skipped before any copying.
        let (cam, reach) = (info.camera, rt.engine.entity_reach_sq());
        for m in game.mobs.iter().filter(|m| !m.removed) {
            let pos = [m.position.x, m.position.y, m.position.z];
            let d2 = (pos[0] - cam[0]).powi(2) + (pos[1] - cam[1]).powi(2) + (pos[2] - cam[2]).powi(2);
            if d2 > reach {
                continue;
            }
            let sprinting = m.extra.sprinting && m.on_ground;
            let soul = sprinting && on_soul(game, rt, pos);
            let obs = EntityObs {
                id: m.id as i32,
                kind: &m.kind.id,
                pos,
                width: m.width() as f32,
                height: m.height() as f32,
                baby: m.is_baby(),
                crouching: m.extra.crouching,
                invisible: m.is_invisible(),
                alive: m.health > 0.0 && m.death_time == 0,
                on_ground: m.on_ground,
                sprinting: m.extra.sprinting,
                charged: m.extra.charged,
                variant: m.extra.variant,
                armor: [
                    m.extra.head.as_deref(),
                    m.extra.chest.as_deref(),
                    m.extra.legs.as_deref(),
                    m.extra.feet.as_deref(),
                ],
                on_soul_block: soul,
                local: false,
            };
            rt.engine.entity(&obs, &mut in_tag);
        }
        // The player's own body: armor in third person, soul sprinting.
        if let Some(p) = &game.player {
            let pos = [p.position.x, p.position.y, p.position.z];
            let soul = p.sprinting && p.ground.on_ground && on_soul(game, rt, pos);
            let armor = |i: usize| game.armor[i].as_ref().map(|s| &*s.item);
            let obs = EntityObs {
                id: game.remote.as_ref().map_or(i32::MIN, |r| r.entity_id),
                kind: "minecraft:player",
                pos,
                width: 0.6,
                height: if p.shift_key_down { 1.5 } else { 1.8 },
                crouching: p.shift_key_down,
                alive: true,
                on_ground: p.ground.on_ground,
                sprinting: p.sprinting,
                armor: [armor(0), armor(1), armor(2), armor(3)],
                on_soul_block: soul,
                local: true,
                ..EntityObs::default()
            };
            rt.engine.entity(&obs, &mut in_tag);
        }
        rt.engine.end_entities();

        // Rain ripples on loaded, exposed, still water.
        if let Some(p) = &game.player {
            let world = &game.world;
            let facts = game.res.facts();
            let properties = &game.pack.assets.properties;
            let biomes = world_biomes!(game);
            let center = [
                p.position.x.floor() as i32,
                p.position.y.floor() as i32,
                p.position.z.floor() as i32,
            ];
            rt.engine.water(center, |x, z| {
                if !world.loaded(x, z) {
                    return None;
                }
                let top = super::render_tick::motion_blocking_height(world, properties, x, z);
                if top <= world.bottom_y() || (top - center[1]).abs() > 24 {
                    return None;
                }
                let water = facts.get(world.block(x, top - 1, z));
                if &*water.name != "minecraft:water" {
                    return None;
                }
                let above = world.block(x, top, z);
                if !world.is_air(above) && !facts.get(above).name.ends_with(":cave_air") {
                    return None;
                }
                let chunk = world.chunks.get(&(x >> 4, z >> 4))?;
                let biome = biomes.get(chunk.biome(x, top, z));
                Some(WaterColumn {
                    surface_y: (top - 1) as f64 + water.fluid_level as f64 / 9.0,
                    source: water.fluid_level == 8 && !water.fluid_falling,
                    raining: biome.snows_at(x, top, z) == Some(false),
                    water_rgb: biome.water,
                })
            });
        }
        let world = &game.world;
        rt.engine.end_tick(|p| world.light(p[0], p[1], p[2]));
        rt.engine.counters.last_tick_us = micros(start);
    });
}

/// Once a frame: the interpolated draw list for the renderer.
pub(super) fn frame(game: &mut super::Game<'_>) {
    if !game.native_window() {
        return;
    }
    let start = std::time::Instant::now();
    let partial = game.clock.fraction() as f32;
    vh::runtime::with_existing(|rt| {
        rt.engine.frame(partial);
        rt.engine.counters.last_frame_us = micros(start);
    });
}

pub(super) fn listed(mods: &mut Vec<super::native_hooks::NativeMod>) {
    mods.push(super::native_hooks::NativeMod {
        id: vh::MOD_ID.into(),
        title: format!("{} {}", vh::TITLE, vh::VERSION),
        description: vh::DESCRIPTION.into(),
        keys: Vec::new(),
    });
}

pub(super) fn page(game: &mut super::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    if mod_id != vh::MOD_ID || !game.native_window() {
        return;
    }
    vh::runtime::with(config_dir, |rt| buttons.extend(rt.settings_labels()));
}

pub(super) fn click(game: &mut super::Game<'_>, mod_id: &str, button: usize) {
    if mod_id != vh::MOD_ID || !game.native_window() {
        return;
    }
    vh::runtime::with(config_dir, |rt| rt.settings_click(button));
}
