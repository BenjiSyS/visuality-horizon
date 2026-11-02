"""Packs the Blender renders into the shipped atlas, losslessly.

    python tools/pack_atlas.py art mixins/visuality_horizon/engine/data

Reads art/manifests/render_provenance.json, checks every frame (size, RGBA,
not empty, nothing touching the cell border, hash matches the record),
copies each frame's pixels into a deterministic shelf layout with
transparent gutters, and writes atlas.png and atlas.json (the runtime
manifest, with each frame's source file and hash).
"""

import hashlib
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import pngio  # noqa: E402

GUTTER = 2
ATLAS_W = 256
SURFACE = {"water_ring"}


def sha(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def check_frame(anim, fr, w, h, px, cell):
    where = f"{anim['id']} frame {fr['frame']}"
    if (w, h) != tuple(cell):
        raise SystemExit(f"{where}: {w}x{h}, expected {cell}")
    alpha = px[3::4]
    if not any(alpha):
        raise SystemExit(f"{where}: empty")
    for x in range(w):
        for y in (0, h - 1):
            if alpha[y * w + x]:
                raise SystemExit(f"{where}: touches the cell border (clipped)")
    for y in range(h):
        for x in (0, w - 1):
            if alpha[y * w + x]:
                raise SystemExit(f"{where}: touches the cell border (clipped)")


def main():
    art, out = sys.argv[1], sys.argv[2]
    with open(os.path.join(art, "manifests", "render_provenance.json"), encoding="utf-8") as f:
        prov = json.load(f)
    # Shelf packing, biggest cells first, then manifest order: deterministic.
    anims = sorted(enumerate(prov["animations"]), key=lambda ia: (-ia[1]["cell"][1], ia[0]))
    x, y, shelf_h = GUTTER, GUTTER, 0
    placed = {}
    for _, a in anims:
        cw, ch = a["cell"]
        rects = []
        for fr in a["frames"]:
            if x + cw + GUTTER > ATLAS_W:
                x, y = GUTTER, y + shelf_h + GUTTER
                shelf_h = 0
            rects.append([x, y, cw, ch])
            x += cw + GUTTER
            shelf_h = max(shelf_h, ch)
        placed[a["id"]] = rects
    height = y + shelf_h + GUTTER
    side = 1
    while side < height:
        side *= 2
    atlas = bytearray(ATLAS_W * side * 4)
    animations = []
    for a in prov["animations"]:
        frames = []
        for fr, rect in zip(a["frames"], placed[a["id"]]):
            path = os.path.join(art, fr["file"])
            if sha(path) != fr["sha256"]:
                raise SystemExit(f"{fr['file']}: hash differs from the render record; re-render")
            w, h, px = pngio.read(path)
            check_frame(a, fr, w, h, px, a["cell"])
            rx, ry = rect[0], rect[1]
            for row in range(h):
                o = ((ry + row) * ATLAS_W + rx) * 4
                atlas[o:o + w * 4] = px[row * w * 4:(row + 1) * w * 4]
            frames.append({"rect": rect, "duration_ms": fr["duration_ms"], "source": f"{fr['file']}#{fr['sha256'][:16]}"})
        cw, ch = a["cell"]
        animations.append({
            "id": a["id"],
            "orientation": "surface" if a["id"] in SURFACE else "billboard",
            "loop": False,
            "pivot_pixels": [cw // 2, ch // 2],
            "frames": frames,
        })
    os.makedirs(out, exist_ok=True)
    png_path = os.path.join(out, "atlas.png")
    pngio.write(png_path, ATLAS_W, side, atlas)
    manifest = {
        "schema_version": 1,
        "atlas": {
            "path": "atlas.png",
            "width": ATLAS_W,
            "height": side,
            "color_space": "srgb",
            "alpha": "straight",
            "sampling": "nearest",
            "sha256": sha(png_path),
        },
        "animations": animations,
        "provenance": {
            "blender": prov["blender"],
            "script_sha256": prov["script_sha256"],
            "records": "art/manifests/render_provenance.json",
        },
    }
    with open(os.path.join(out, "atlas.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(manifest, f, indent=1)
        f.write("\n")
    print(f"atlas {ATLAS_W}x{side}, {sum(len(a['frames']) for a in animations)} frames, {os.path.getsize(png_path)} bytes")


main()
