# Visuality for Horizon

Small pixel particles for [Horizon](https://github.com/horizon-rust/horizon), the Rust
reimplementation of Minecraft: Java Edition. It is a client-side native mod: an original Rust
implementation of the effects of PinkGoosik's [Visuality](https://github.com/PinkGoosik/visuality)
(MIT), with its own art made in Blender.

Not made or endorsed by Visuality's author, Mojang, Microsoft or the Horizon project.

## Effects

| Effect | What you see | Trigger |
|---|---|---|
| Hit fragments | Bone chips (skeleton, stray, skeleton horse), dark chips (wither skeleton), feathers in the chicken's own colours, emerald chips (villager) | A damage event the server sends for that mob, with an attacker |
| Slime blobs | Chunky blobs in three sizes, tinted (default `#88FF79`) | A normal slime landing; replaces the vanilla slime crumbs. Magma and sulfur cubes are left alone |
| Charged creeper sparks | Short electric forks around the body | A live powered creeper, about one chance in 20 per tick |
| Amethyst sparkles | Violet glints | Full-grown amethyst clusters only (buds optional) |
| Armor glints | Gold and diamond glints at the worn slot | Visible players and mobs. Your own armor shows in third person only |
| Ore glints | Glints on uncovered faces of gold, diamond and emerald ore | Never on fully buried ore |
| Soul wisps | Rising cyan wisps | Above uncovered soul sand and soul soil, and when sprinting on them |
| Rain ripples | Flat expanding rings on the water surface | Rain falling on still, uncovered source water in the overworld |

## Install

1. In the Horizon launcher, open **Mods**, pick the instance, and install `visuality_horizon`.
   You can also use `horizon-launcher native-build visuality_horizon-1.0.2.hnmod --trust-native-code`.
2. Confirm the native-code warning and build. The first native build downloads Rust and compiles the
   whole game, which can take 20 to 30 minutes. Later builds are faster.
3. Play. Settings are in **Options > Mods > Visuality**.

The `.hnmod` is the only thing to install: the art is compiled in, and there is no resource pack,
no Blender and no download at run time. It is a source bundle that the launcher compiles on your
computer. It is not a ready-made executable.

## Settings

- Master switch, quality preset (Low by default, Balanced, High), particle amount, effect distance,
  and reduced motion and flashes.
- One page per effect: on/off, amount, slime colour, water search radius, ripples per tick, biome
  water colour, and soul ambient and sprinting wisps.
- Advanced: particle budget, new particles per tick, whether to follow the game's Particles option,
  and what Minimal particles does.
- Lists (hit mappings, shiny armor items or `#tags`, ore block tags) live in
  `config/visuality_horizon.toml` in the game folder, with **Reload settings file** in game.
  Invalid entries are reported and skipped. A broken file is kept as `.invalid`, and the defaults
  are used.

## Compatibility

- **Pinned** to Horizon commit `c9d474dd9116e84c243f8992d75c11314df56f06` (Nightly 2026-11-01,
  source tag `nightly-source/2026-11-01-c9d474d`), Minecraft 26.3, horizon-mixins `a2af5c0`. It hooks
  private client functions as well as stable hook points, so it does not claim a hooks-only API
  version. Other Horizon commits need a rebuild and retest.
- **Client only.** Nothing is sent to the server and nothing is needed there; other players see the effects
  only if they have the mod. Tested so far in single player on Windows; LAN and server play use the same
  client paths but are not tested yet.
- Platforms: see [docs/VALIDATION.md](docs/VALIDATION.md) for what has actually been tested.

## Building from source

```sh
cargo test --workspace --locked                      # logic, config, assets, packaging rules
python tools/package.py                              # dist/visuality_horizon-<version>.hnmod (reproducible)
horizon-launcher native-build mixins/visuality_horizon --instance <test instance> --trust-native-code
```

Art: `blender --background --factory-startup --python art/blender/render_assets.py -- --out art` with
Blender 5.2.3, then `python tools/pack_atlas.py art mixins/visuality_horizon/engine/data`. See
[docs/ART_DIRECTION.md](docs/ART_DIRECTION.md).

## Licence

MIT for code and art ([LICENSE](LICENSE)). Visuality's MIT notice is kept in
[docs/THIRD_PARTY_NOTICES.md](docs/THIRD_PARTY_NOTICES.md).
