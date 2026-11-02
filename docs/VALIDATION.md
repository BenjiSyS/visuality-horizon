# Validation record

Every row says what was actually run, where, and with what result. "Not run" rows are open.

## Environment

| Item | Value |
|---|---|
| Machine | Windows 11 Home 10.0.26200, x86_64 laptop |
| GPU | Intel UHD Graphics 620 (integrated), Vulkan, Intel driver |
| Launcher | horizon-launcher 0.2.37 (Windows x64) |
| Horizon | Nightly 2026-11-01, `c9d474dd9116e84c243f8992d75c11314df56f06`, Minecraft 26.3 |
| horizon-mixins | `a2af5c0dd4117d51ecd70066ba7a00b4a2c10dab` (the launcher's `MIXINS_REV`) |
| Rust | 1.99.1 (the launcher's own windows-gnu toolchain for game builds; rustup 1.99.1 for `cargo test`) |
| Blender | 5.2.3 LTS, build `e13b728c6880` |
| Test instance | `visuality-test` (an isolated launcher instance), world `vh-fixture` (creative). Render distance 12, simulation 8 |

## Automated checks

| Check | Result |
|---|---|
| `cargo fmt --all --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` (with the wgpu renderer) | pass |
| `cargo test` | 53 passed: config (CFG-01..10), pool limits (LIM-01..03), every family's rules and negative controls, lifecycle (LIFE-01..06), assets, settings menu |
| `python tools/pack_atlas.py` | 69 frames checked (size, RGBA, not empty, no pixel on the cell border, hash matches the render record). Caught two real clipping bugs (medium slime stretch frame, soul base), both fixed in Blender and re-rendered |
| `python tools/package.py` | two packs byte-identical, 32 entries, registry path rules satisfied |
| Real build: `horizon-launcher native-build mixins/visuality_horizon --instance visuality-test --trust-native-code` | pass. First full build 37 min 44 s, later incremental builds 15 to 21 min, 0 warnings after the visibility fix |

| Player route: the released `.hnmod` unpacked into a clean folder, then `native-build` | pass, 14 min 31 s (incremental; the launcher built horizon-mixins `a2af5c0` itself as the bundle names it) |
| Registry checker: `cargo horizon-mod check visuality_horizon-1.0.0.hnmod` (built from the pinned source) | pass: native-1/c9d474d/a2af5c0, 70145 bytes, SHA-256 `a0fbf91a5b4d3aeb52e42fdd59a4d23d359dfe7074dbdea6a68af9dcc6bcc47b`, unsigned |

## In-game checks (natural events, single player, Windows, UHD 620)

Short-lived effects were captured with `/tick rate 2` to `5`, because a screenshot takes about 0.9 s to save.

| Case | What was done | Result | Evidence |
|---|---|---|---|
| Mod listed and settings | Options > Mods | "Visuality 1.0.0: ON", Settings page, all pages navigable; Reload settings file works | counters read through the UI |
| ORE-02 / ORE-03 | Diamond and gold ore wall | Glints on open faces; gold tint on gold, cyan on diamond | screenshots in session |
| VIS-01 depth | Stone pillar 1.5 blocks in front of the ore wall; trace mode alternates the depth test | Depth on: no glint over the pillar or leaves. Depth off (debug only): glints show through | `evidence/vis01_*` |
| HIT-05 | Hit a villager | Emerald chips on its body | `evidence/hit05_villager_emerald_slowmo.png` |
| HIT-03 | Hit a wither skeleton | Dark wither-bone chips | `evidence/hit03_wither_bone_and_chg01_spark_slowmo.png` |
| CHG-01 | Powered creeper | Electric spark beside it; counters show 101 charge particles in about 3 minutes with 1 powered creeper | same image |
| SLM-01 | Slimes of size 1 and 4 dropped from 3 blocks | Green blobs at landing; 38 landings counted | `evidence/slm01_slime_blobs_arm01_glint.png` |
| ARM-01 | Zombie with gold and diamond armor | Glints at the armor; 162 armor glints counted | same image |
| WAT-01 | Rain over a 6×3 source pool | Flat rings on the surface | `evidence/wat01_rain_rings.png` |
| HIT-04 | Hit a live chicken | Feather at the chicken | `evidence/hit04_chicken_feather_slowmo.png` |
| LIFE-01 | Master switch off (left off by the benchmark script) | Counters froze: no tick work, no spawns, no draws, while the game ran | counters before and after |
| AME-01 | Full amethyst cluster in the fixture | 96 amethyst sparkles counted in one session; the large bud next to it is not a source (unit test AME-02) | counters |
| BENCH-MIX | ABBA on/off, 3 pairs of 20 s | No regression measurable; noise about 9% (PERFORMANCE.md) | `bench/BENCH-MIX-*.json` |
| Hurt signal | `DamageEvent` canonical | Hurts counted; damage without an attacker (sun fire, drowning) gives no debris | counters |
| Performance snapshot | ore wall with 8 to 12 live particles | tick 71 to 113 µs, frame 5 to 15 µs, draw encode 104 to 183 µs (CPU) | settings counters |

## Findings fixed during testing

- Particles were drawn correctly but too small to read: 8 to 14 px at 1440p. Every quad was enlarged (ART_DIRECTION.md) and the glint timing now dwells on its peak.
- Mod log lines were dropped because Horizon's logger keeps only `horizon*` targets. They now log under `horizon::visuality_horizon`.
- `pub(crate)` glue functions raised `private_interfaces` warnings, so they are now `pub(super)`.

## Open (not yet run or not qualified)

- **GPU out of memory:** the first session crashed with `wgpu error: Out of Memory` in Horizon's terrain upload (`Flat::add`). That session had render distance 128 on the UHD 620. The mod's own GPU allocation is about 370 KB. The same scene has not yet been run on an unmodded build, so the mod is not proven innocent.
- HIT-01/02 visual (skeleton, stray), the cold and warm feather palettes on screen, SLM-04 toggle-off, SOUL on screen (16 soul wisps counted), WAT-02..07, LIFE-01..10 in game, NET-01..05 (LAN, dedicated server, a second client), benchmarks (BENCH-*), and long runs.
- Linux x86_64 and macOS arm64: not built or run here. Intel macOS: the launcher ships no toolchain for it. Dedicated servers: native mods are not built there yet (Horizon docs), and the mod is client-only anyway.
