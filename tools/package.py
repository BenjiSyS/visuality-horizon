#!/usr/bin/env python3
"""Builds the one player package, dist/visuality_horizon-<version>.hnmod.

    python tools/package.py

The archive is an explicit allowlist: `horizon-native.json` and the bundle
under `mixins/visuality_horizon/` (its manifest and helpers, the engine
crate's Cargo.toml and sources, the packed atlas and its manifest, the
licence and notices). Entries are sorted, dated 2026-01-01 and mode 0644,
so the same sources give the same bytes. It is packed twice into separate
paths and the hashes compared; the result is checked with the same rules as
Horizon's `horizon_mods::index::native::Bundle::read`.
"""

import hashlib
import json
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MOD_ID = "visuality_horizon"
BUNDLE = ROOT / "mixins" / MOD_ID
MAX_ENTRIES = 512
MAX_UNPACKED = 64 * 1024 * 1024


def allowlist():
    files = []
    for p in sorted(BUNDLE.iterdir()):
        if p.is_file() and p.suffix in (".rs", ".toml"):
            files.append(p)
    for name in ("LICENSE", "NOTICE"):
        files.append(BUNDLE / name)
    engine = BUNDLE / "engine"
    files.append(engine / "Cargo.toml")
    files.extend(sorted((engine / "src").rglob("*.rs")))
    files.append(engine / "data" / "atlas.png")
    files.append(engine / "data" / "atlas.json")
    for f in files:
        if not f.is_file() or f.is_symlink():
            raise SystemExit(f"package: missing or not a regular file: {f}")
    return files


def in_data(rest):
    parts = rest.split("/")
    return len(parts) >= 2 and (parts[0] == "data" or (len(parts) >= 3 and parts[1] == "data"))


def text(rest):
    parts = rest.split("/")
    name = parts[-1]
    return len(parts) <= 2 and (
        any(name.startswith(p) for p in ("README", "LICENSE", "LICENCE", "COPYING", "NOTICE"))
        or name == ".gitignore"
    )


def allowed(rest):
    return rest.endswith(".rs") or rest.endswith(".toml") or in_data(rest) or text(rest)


def manifest():
    compat = json.loads((ROOT / "compatibility.json").read_text(encoding="utf-8"))
    mixin = tomllib.loads((BUNDLE / "horizon-mixin.toml").read_text(encoding="utf-8"))
    mod = mixin["mod"]
    if mod["id"] != MOD_ID:
        raise SystemExit("package: horizon-mixin.toml id differs")
    if mixin["compatibility"]["horizon_revision"] != compat["horizon_revision"]:
        raise SystemExit("package: horizon-mixin.toml and compatibility.json pin different revisions")
    if "hooks" in compat:
        raise SystemExit("package: this mod hooks private functions; it must not declare a hooks API")
    cargo = tomllib.loads((BUNDLE / "engine" / "Cargo.toml").read_text(encoding="utf-8"))
    if cargo["package"]["version"] != mod["version"]:
        raise SystemExit("package: engine crate and mod versions differ")
    return {
        "schema": 1,
        "id": MOD_ID,
        "version": mod["version"],
        "game": compat["game"],
        "horizon": compat["horizon_revision"],
        "mixins": compat["horizon_mixins_revision"],
    }


def pack(out):
    m = manifest()
    entries = {"horizon-native.json": (json.dumps(m, indent=2, sort_keys=True) + "\n").encode()}
    for f in allowlist():
        name = f.relative_to(ROOT).as_posix()
        data = f.read_bytes()
        if f.suffix in (".rs", ".toml", ".json") or f.name in ("LICENSE", "NOTICE"):
            # Same bytes on every OS whatever Git did to line endings.
            data = data.replace(b"\r\n", b"\n")
        entries[name] = data
    check(entries, m)
    out.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as z:
        for name, content in sorted(entries.items()):
            info = zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            z.writestr(info, content, compresslevel=9)
    return m, entries


def check(entries, m):
    if len(entries) > MAX_ENTRIES:
        raise SystemExit(f"package: {len(entries)} entries, limit {MAX_ENTRIES}")
    total = sum(len(v) for v in entries.values())
    if total > MAX_UNPACKED:
        raise SystemExit(f"package: {total} bytes unpacked, limit {MAX_UNPACKED}")
    prefix = f"mixins/{m['id']}/"
    folded = set()
    for name in entries:
        if name in ("horizon-native.json",):
            continue
        if "\\" in name or ":" in name or name.startswith("/") or any(p in ("", ".", "..") for p in name.split("/")):
            raise SystemExit(f"package: invalid path {name}")
        if not name.startswith(prefix) or not allowed(name[len(prefix):]):
            raise SystemExit(f"package: the registry would refuse {name}")
        if name.lower() in folded:
            raise SystemExit(f"package: {name} collides with another entry on Windows")
        folded.add(name.lower())
        lowered = name.lower()
        for bad in (".blend", ".env", "secret", "token", "target/", ".git/"):
            if bad in lowered:
                raise SystemExit(f"package: {name} looks like it does not belong in a player package")
    for name, data in entries.items():
        if name.endswith((".rs", ".toml", ".json")):
            text_ = data.decode("utf-8")
            for marker in ("C:\\Users", "/home/", "/Users/"):
                if marker in text_:
                    raise SystemExit(f"package: {name} contains an absolute local path ({marker})")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    first = ROOT / "target" / "package-a" / "x.hnmod"
    second = ROOT / "target" / "package-b" / "x.hnmod"
    m, entries = pack(first)
    pack(second)
    if sha(first) != sha(second):
        raise SystemExit("package: two packs of the same sources differ")
    dist = ROOT / "dist"
    dist.mkdir(exist_ok=True)
    final = dist / f"{m['id']}-{m['version']}.hnmod"
    final.write_bytes(first.read_bytes())
    digest = sha(final)
    (dist / "SHA256SUMS").write_text(f"{digest}  {final.name}\n", encoding="utf-8", newline="\n")
    listing = {
        "file": final.name,
        "sha256": digest,
        "size": final.stat().st_size,
        "entries": len(entries),
        "unpacked_bytes": sum(len(v) for v in entries.values()),
        "manifest": m,
        "files": sorted(entries),
    }
    (dist / "package-listing.json").write_text(json.dumps(listing, indent=1) + "\n", encoding="utf-8", newline="\n")
    print(json.dumps({k: listing[k] for k in ("file", "sha256", "size", "entries", "unpacked_bytes")}, indent=1))


if __name__ == "__main__":
    main()
