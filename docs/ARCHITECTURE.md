# Architecture and integration map

Pinned source: Horizon `c9d474dd9116e84c243f8992d75c11314df56f06` (Nightly 2026-11-01), horizon-mixins
`a2af5c0`, `NATIVE_HOOKS_API` 80, Rust 1.99.1 (the launcher's), wgpu 30.0.1.

## Decision

Horizon's particle engine (`horizon-client-render/src/particles.rs`) is vanilla's: a mod particle there
is a named sprite set from a resource pack's `particles/*.json` with vanilla behaviours. It has no
surface-oriented quad (rain rings), no per-particle tint with custom lifetime curves, and no way to
ship sprites inside a `.hnmod` without a resource pack. `GameLevel::native_send_particles` is
server-side. So the mod keeps its own bounded particle pool, stepped by the game's own client tick
(20 Hz, paused with the game), and draws it in one pass after the world through the stable
`native_world_drawn` render hook point, depth-tested in the shader against the world's reversed depth.
Vanilla particles are untouched except for the scoped slime substitution.

## Code layout

| Part | Where | Owns |
|---|---|---|
| Engine crate `visuality-horizon-engine` | `mixins/visuality_horizon/engine/` (bundle `[[crate]]`, dependent `horizon-client-render`) | config, rules, emitters, pool, assets, wgpu drawing, settings menu, counters |
| Render glue (pub module) | injected into `crates/horizon-client-render/src/gfx/native_render.rs` as `visuality_horizon` | re-exports the engine to the game crate; world-drawn and render-needs entry points |
| Game glue | injected into `crates/horizon-client/src/game.rs` as `visuality_horizon` | copies client state into the engine; settings page |
| Helpers | `mixins/visuality_horizon/*.rs` | one-line hook adapters |

The engine is plain Rust with `cargo test` coverage (no GPU, no account needed). The `gpu` feature,
on by default, adds the wgpu renderer.

## Integration points

| Purpose | Target (pinned commit) | Kind | Stable? | Thread / side | Data in | Original body |
|---|---|---|---|---|---|---|
| Client tick | hook `tick` v1 (`Game::native_tick`) | HEAD | stable hook | window's game only (`native_window()` checked) | entities, player, weather, light, tags | empty hook |
| Frame | hook `frame` v1 | HEAD | stable | window | partial tick | empty hook |
| Draw | hook `world_drawn` v1 (`gfx::native_render::native_world_drawn`) | HEAD | stable | render (same window thread) | device, queue, encoder, colour, depth, matrices | empty hook |
| Depth request | hook `render_needs` v1 | HEAD | stable | render | sets `world_depth` while particles live | empty hook |
| Mods list / settings | hooks `mods`, `settings`, `settings_click` v1 | HEAD | stable | window | | empty hooks |
| Hurt | `game::remote::Game::handle_packet` | HEAD, by reference | private, pinned | window | `DamageEvent{id, cause, direct}` (canonical), `HurtAnimation{id}` (counted, ignored) | runs unchanged |
| Id reuse | `game::remote::Game::spawn_entity`, `remove_entity` | HEAD | private | window | entity id | unchanged |
| Session | `game::remote::Game::leave_server`, `enter_remote_dimension` | HEAD | private | window | | unchanged |
| Ambient blocks | `animate_tick::block_animate_tick` | HEAD | private | window, once per sampled block (1334 a tick) | state, position, loaded world, block facts | unchanged |
| Slime | `mobs::cube::client_tick` | cancellable HEAD | private | window's copy of the mob | species, on-ground flag, size, width | runs unchanged unless a normal slime lands with Slime blobs on; then the mod sets the same landing flag and skips only the crumb splash |

In single player and on LAN the window's game is a client of the integrated server through an in-memory
pipe, so the same packet and entity paths serve every mode. The engine lives in a thread-local on
the window's thread, so a server-thread `Game` can never share or double its state.

## Lifecycle

`DisabledReady` (master switch or the launcher's Mods toggle off: `Game::native_on`), `ReadyNoWorld`,
`ActiveWorld`, `VisualFailure` (atlas invalid: one log line, nothing emitted or drawn). Leaving a server
or changing dimension starts a new session: particles, pending hurts, cooldowns and block-class caches
are cleared. GPU objects are rebuilt when the device changes; the bind group only when the depth view
changes; one pipeline per colour format.

## Hit identity

The damage event carries no hit id. Policy: the damage event is canonical, the hurt animation is
counted and ignored, and a mob gets at most one burst per `cooldown_ticks` (default 10, the reference's
throttle). Cooldowns are keyed by entity id and dropped when the id is added or removed, and when the
session changes. A real second hit inside the window is throttled on purpose; it is not claimed to be
a duplicate packet. Attacker equipment is unknown on the client, so bursts are two to four fragments
times the amounts, with ten at most.
