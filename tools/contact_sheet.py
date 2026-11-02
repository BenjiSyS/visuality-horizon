"""Review sheets from the Blender renders: every frame of every animation at
1x and enlarged (nearest), over dark, light, saturated and checker grounds.

    python tools/contact_sheet.py art            -> art/review/contact_sheet.png

Review output only; nothing here is shipped.
"""

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import pngio  # noqa: E402

GROUNDS = [(24, 24, 28), (236, 236, 230), (70, 120, 200), (110, 160, 70)]
ZOOM = 6
PAD = 4


def main():
    art = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else "art")
    with open(os.path.join(art, "manifests", "render_provenance.json"), encoding="utf-8") as f:
        prov = json.load(f)
    anims = prov["animations"]
    cell = 32 * ZOOM
    max_frames = max(len(a["frames"]) for a in anims)
    band = max_frames * (cell + PAD) + PAD
    width = len(GROUNDS) * band
    row_h = cell + 32 + PAD * 2
    height = len(anims) * row_h
    img = bytearray(width * height * 4)

    def put(x, y, rgb, a=255):
        if 0 <= x < width and 0 <= y < height:
            o = (y * width + x) * 4
            img[o:o + 4] = bytes((*rgb, a))

    for row, anim in enumerate(anims):
        y0 = row * row_h + PAD
        for g, ground in enumerate(GROUNDS):
            x_band = g * band
            for yy in range(y0 - PAD, y0 + row_h - PAD):
                for xx in range(x_band, x_band + band):
                    put(xx, yy, ground)
            for i, fr in enumerate(anim["frames"]):
                w, h, px = pngio.read(os.path.join(art, fr["file"]))
                x0 = x_band + PAD + i * (cell + PAD)
                for sy in range(h):
                    for sx in range(w):
                        o = (sy * w + sx) * 4
                        a = px[o + 3] / 255.0
                        rgb = tuple(int(px[o + k] * a + ground[k] * (1 - a)) for k in range(3))
                        # Enlarged by repetition.
                        for dy in range(ZOOM * 32 // h):
                            for dx in range(ZOOM * 32 // w):
                                put(x0 + sx * (ZOOM * 32 // w) + dx, y0 + sy * (ZOOM * 32 // h) + dy, rgb)
                        # And at 1x below it.
                        put(x0 + sx, y0 + cell + 6 + sy, rgb)
    out = os.path.join(art, "review", "contact_sheet.png")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    pngio.write(out, width, height, img)
    print(out)


main()
