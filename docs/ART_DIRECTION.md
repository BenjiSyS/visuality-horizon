# Art direction and production record

All shipped art is rendered by Blender 5.2.3 LTS (build `e13b728c6880`, official windows-x64 zip,
SHA-256 checked against `blender-5.2.3.sha256`) from the scenes `art/blender/*.blend`, which
`art/blender/render_assets.py` builds and saves. No image was painted, generated, traced or imported.
The packer (`tools/pack_atlas.py`) only copies rendered rectangles into the atlas; `tools/contact_sheet.py`
only composites renders over flat review grounds.

## Stage

- `BLENDER_EEVEE`, 1 render sample (binary coverage, so no soft halos), filter size 0.01 (the
  property's hard minimum is 0.0, but a small positive value is set instead of zero).
- Orthographic camera `PixelCamera` at (0, 0, 5) looking down -Z, `ortho_scale` 1.0. One pixel is 1/cell.
- Transparent film, PNG RGBA 8-bit, Standard view transform, look None, exposure 0, gamma 1.
- Flat emission materials from the palette, converted from sRGB hex to scene-linear in the script.
- Stepped (constant) keys: each frame is an authored pose. The pivot is the cell centre.

## Cells, frames and scale

| Animation | Cell | Frames | Runtime quad (blocks) | Lifetime |
|---|---|---|---|---|
| bone_a/b, wither_bone_a/b | 16 | 1 | 0.18–0.28 | 0.35–0.65 s |
| feather, feather_warm, feather_cold | 16 | 1 | 0.22–0.34 | 0.55–1.0 s |
| emerald | 16 | 1 | 0.16–0.24 | 0.30–0.50 s |
| slime_small / medium | 16 | 6 | 0.18 / 0.30 | 0.25–0.55 s |
| slime_large | 32 | 6 | 0.42 | 0.25–0.55 s |
| sparkle, sparkle_amethyst | 16 | 6 | 0.20–0.32 | 0.20–0.45 s |
| charge_a/b/c | 32 | 4 | 0.32–0.50 | 0.40–0.55 s |
| soul_ambient / soul_sprint | 32 | 8 / 5 | 0.32–0.45 / 0.28–0.36 | 0.6–1.2 s / 0.2–0.45 s |
| water_ring | 32 | 6 | 0.75 (surface) | 0.25–0.35 s |

In-game review (2026-11-02, 1440p, UHD 620) found the brief's starting sizes unreadable (8 to 14 px). The table holds the enlarged sizes adopted after that review.
Frame durations in the manifest are relative weights over the particle's lifetime. Static fragments get
their motion and rotation from the runtime, not from the sprite. The last fifth of every lifetime fades.

## Palette and tinting

Slime, sparkle and water ring are authored neutral (greys and white) and tinted at runtime: slime by the
configured colour, sparkles by material (gold `#FFE48A`, diamond `#B8F6FF`, emerald `#A8FFB8`), and
rings by a lightened biome water colour. Bone, wither bone, feathers, emerald, amethyst, charge and soul
carry their own palettes (`PALETTE` in the script). Every sprite has a darker outline layer, so it
reads on bright and dark ground.

## Review record (contact sheet `art/review/contact_sheet.png`, 1x and 6x, four grounds)

| Family | Observation | Result |
|---|---|---|
| Bone / wither bone | Two distinct angular chip silhouettes. The wither palette is a dark charcoal with a lighter top facet: readable on light and saturated ground, low contrast on near-black ground (night caves). | Accepted; dark-ground contrast noted |
| Feathers | A quill and a notched vane read as a feather at 16 px. Normal, warm and cold palettes are clearly distinct. | Accepted |
| Emerald | The first version read as a house shape (a pentagon). Remodelled as a tall hexagonal chip with dark left facets and one glint. | Rejected, then reworked and accepted |
| Slime | Neutral, stretch, compact, settle, shrink, drop. The stretch frame of the medium blob touched the cell edge (caught by the packer's border check), so the radii were reduced and re-rendered. | Reworked and accepted |
| Sparkles | A six-phase star with a short peak; the amethyst variant has a violet edge. The neutral one is near-invisible on white ground until tinted, which is by design. | Accepted |
| Charge | Three seeds, each a bolt with a fork; frames go onset, full, fork, break-up. No full-cell flashes. | Accepted |
| Soul | The first version read as a triangle fin. Remodelled as a teardrop plume with a curl and two motes. The bottom touched the cell, so the base was raised and re-rendered. | Reworked and accepted |
| Water ring | A 16-segment ring expands and thins around a fixed centre, with a lit upper-left quarter. Faint on white ground; it is drawn on water. | Accepted |

Still to do: the in-game look must be judged from real captures (see VALIDATION.md). A contact sheet
does not prove in-game rendering.
