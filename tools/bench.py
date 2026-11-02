#!/usr/bin/env python3
"""Paired on/off benchmark of Visuality in a running test game.

    python tools/bench.py --ctl PATH/horizon-ctl.exe --socket PATH/game-control.json \
        --scenario BENCH-MIX --seconds 20 --pairs 3 --out docs/bench

The game must already be in the fixture world with the camera placed. The
script runs ABBA-ordered captures (mod on, mod off, off, on, ...), toggling
Visuality's master switch through its settings page between captures, after a
warm-up. It writes every reply of `horizon-ctl bench` verbatim to a raw JSON
file and a summary computed from them. Nothing is uploaded.
"""

import argparse
import datetime
import json
import statistics
import subprocess
from pathlib import Path


def ctl(a, *args):
    out = subprocess.run([a.ctl, "--socket", a.socket, *args], capture_output=True, text=True, timeout=600)
    try:
        return json.loads(out.stdout)
    except json.JSONDecodeError:
        raise SystemExit(f"horizon-ctl {' '.join(args)}: {out.stdout[:200]} {out.stderr[:200]}")


def labels(a):
    return [w.get("label", "") for w in ctl(a, "ui").get("widgets", [])]


def set_master(a, on):
    ctl(a, "pause")
    ctl(a, "click-button", "Options...")
    ctl(a, "click-button", "Mods")
    ctl(a, "click-button", "Settings")
    for _ in range(4):
        if any(l.startswith("Visuality: ") for l in labels(a)):
            break
        ctl(a, "click-button", "Back")
    current = next(l for l in labels(a) if l.startswith("Visuality: "))
    if current.endswith("On") != on:
        ctl(a, "click-button", "Visuality: ")
    for _ in range(4):
        ctl(a, "close")
    ctl(a, "resume")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ctl", required=True)
    ap.add_argument("--socket", required=True)
    ap.add_argument("--scenario", required=True)
    ap.add_argument("--seconds", type=float, default=20)
    ap.add_argument("--pairs", type=int, default=3)
    ap.add_argument("--warmup", type=float, default=10)
    ap.add_argument("--out", default="docs/bench")
    a = ap.parse_args()
    state = ctl(a, "state")
    runs = []
    order = []
    for i in range(a.pairs):
        order += [True, False] if i % 2 == 0 else [False, True]
    set_master(a, True)
    ctl(a, "bench", str(a.warmup))
    for n, on in enumerate(order):
        set_master(a, on)
        ctl(a, "wait", "40")
        reply = ctl(a, "bench", str(a.seconds))
        runs.append({"run": n, "mod_enabled": on, "reply": reply})
        print(n, "on" if on else "off", json.dumps(reply)[:160])
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    meta = {
        "scenario": a.scenario,
        "seconds": a.seconds,
        "pairs": a.pairs,
        "warmup": a.warmup,
        "order": ["on" if o else "off" for o in order],
        "renderer": state.get("renderer"),
        "render_distance": state.get("render_distance"),
        "display": state.get("display"),
    }
    raw = out / f"{a.scenario}-{stamp}.json"
    raw.write_text(json.dumps({"meta": meta, "runs": runs}, indent=1) + "\n", encoding="utf-8")
    print(raw)


if __name__ == "__main__":
    main()
