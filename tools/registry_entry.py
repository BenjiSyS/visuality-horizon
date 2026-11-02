#!/usr/bin/env python3
"""Writes dist/registry/mods/visuality_horizon.toml, the horizon-mods entry
for the package in dist/, in the registry's current format.

    python tools/registry_entry.py --owner GITHUB_LOGIN --source https://github.com/OWNER/REPO [--published YYYY-MM-DD]

The entry names the GitHub account that will own it and the public source
repository the .hnmod was built from. Nothing is uploaded.
"""

import argparse
import datetime
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

SUMMARY = "Small pixel particles: hit fragments, slime blobs, creeper sparks, glints, soul wisps and rain ripples."
DESCRIPTION = """An original client-side Rust implementation of the effects of PinkGoosik's Visuality (MIT), with its own art made in Blender.

Effects
Hit fragments: bone chips from skeletons, strays and skeleton horses, dark chips from wither skeletons, feathers in each chicken variant's colours, emerald chips from villagers.
Slime blobs: chunky blobs in three sizes when a slime lands, in a colour you choose (magma cubes are left alone).
Charged creeper sparks, amethyst sparkles on full clusters, armor glints on gold and diamond armor, ore glints on uncovered faces only, soul wisps over soul sand and soil and when sprinting on them, and flat rain ripples on open water.

Low preset by default, for integrated GPUs; Balanced and High add density. Settings in Options > Mods > Visuality, lists in config/visuality_horizon.toml.
Client only: it sends nothing to the server and needs nothing installed there. Tested in single player on Windows; LAN, servers, Linux and macOS are not tested yet (see docs/VALIDATION.md).
Pinned to Horizon c9d474d (Nightly 2026-11-01): it hooks private client functions, so it builds on that commit only.
Not made or endorsed by Visuality's author."""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--owner", required=True)
    ap.add_argument("--source", required=True)
    ap.add_argument("--author", default=None)
    ap.add_argument("--published", default=datetime.date.today().isoformat())
    a = ap.parse_args()
    listing = json.loads((ROOT / "dist" / "package-listing.json").read_text(encoding="utf-8"))
    m = listing["manifest"]

    def q(s):
        return json.dumps(s, ensure_ascii=False)

    text = f'''id = "{m["id"]}"
kind = "native-mod"
name = "Visuality"
summary = {q(SUMMARY)}
description = """
{DESCRIPTION}"""
authors = [{q(a.author or a.owner)}]
owners = [{q(a.owner)}]
license = "MIT"
source = {q(a.source)}
icon = "icons/{m["id"]}.png"

[[versions]]
version = "{m["version"]}"
published = "{a.published}"
game = "{m["game"]}"
file = "{listing["file"]}"
sha256 = "{listing["sha256"]}"
changelog = "First release, for Horizon Nightly 2026-11-01 (c9d474d)."
'''
    out = ROOT / "dist" / "registry" / "mods" / f"{m['id']}.toml"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(text, encoding="utf-8", newline="\n")
    print(out)


if __name__ == "__main__":
    main()
