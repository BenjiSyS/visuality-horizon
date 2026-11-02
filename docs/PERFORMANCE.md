# Performance

Reference machine: Windows 11 laptop, Intel UHD Graphics 620 (integrated), Vulkan. 2560×1440,
render distance 12, Low preset (the default). Build: native-c9d474d (performance profile).

## BENCH-MIX: the fixture lane

The fixture holds a powered creeper, an armored zombie, an amethyst cluster, a pool, ore walls and
mapped mobs. Six captures of 20 s each, ABBA-ordered (on, off, off, on, on, off), after a 10 s
warm-up. The mod's master switch was toggled between captures through its settings page. Raw replies
from `horizon-ctl bench` are in [`BENCH-MIX-20261102-172142.json`](bench/BENCH-MIX-20261102-172142.json).

| Metric (median of 3) | Mod on | Mod off |
|---|---|---|
| Frame p50 | 29.17 ms | 30.66 ms |
| Frame p95 | 44.48 ms | 47.11 ms |
| Frame p99 | 56.54 ms | 59.88 ms |
| GPU p50 | 27.25 ms | 27.84 ms |

Run-to-run spread with the mod off: frame p50 30.2 to 32.9 ms (about 9%). The on/off difference
(on was 5% faster) is inside that noise. **Conclusion:** no regression is measurable at this noise
level. These runs cannot show the brief's target of a median increase of 3% or less, because the
machine's noise is larger than that. That would need longer captures and a quieter machine.

## The mod's own cost (in-game counters, Low)

| Stage | Measured |
|---|---|
| Client tick (emitters, entity pass, water sampling, pool step) | 20 to 31 µs after the distance cut-off (`60a1a9b`); 71 to 157 µs before it, with about 200 entities loaded |
| Frame extraction (interpolated draw list) | 3 to 15 µs |
| Draw encoding on the CPU (sort, vertices, upload, one draw call) | 104 to 194 µs |
| GPU memory owned | atlas 256 KB + vertices 96 KB + indices 12 KB + uniforms |
| Draw calls | 1 per frame while particles are live, 0 otherwise |

The tick was above the brief's goal of 0.10 ms at first. Skipping entities beyond the emission distance plus 3 blocks, before copying anything, brought it to 20 to 31 µs. That is measured after 1.0.0 and goes into the next release.

## Limits

Global active particles 128 (Low), 24 admitted per tick, a queue of 256, hard ceilings 1024. The
pool and queue never grow past these (unit tests LIM-01 to LIM-03, plus a 400-tick stress test with
all eight families saturated).
