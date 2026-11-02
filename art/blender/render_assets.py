"""Visuality for Horizon: builds the particle scenes in Blender and renders them.

Run with the pinned Blender (see art/blender/BLENDER_VERSION):

    blender --background --factory-startup --python art/blender/render_assets.py -- \
        --out art [--only water_ring] [--smoke]

Every shipped pixel comes from these scenes: low-poly meshes with flat
emission materials, an orthographic camera looking down -Z at the XY plane,
one render sample (hard pixel edges), Standard view transform. The script
saves one editable .blend per family under art/blender/, renders each
animation's frames to art/renders/<animation>/<frame>.png, and writes
art/manifests/render_provenance.json. It stops with an error on any
unsupported setting instead of rendering a broken batch.
"""

import hashlib
import json
import math
import os
import sys

import bpy

PINNED_VERSION = (5, 2, 3)

# ---------------------------------------------------------------- palette
# sRGB hex values; converted to scene-linear for emission inputs.
PALETTE = {
    "bone_light": "F1EAD4",
    "bone_mid": "D9CFAF",
    "bone_shadow": "A99D7C",
    "bone_line": "6F6550",
    "wither_light": "6A6A70",
    "wither_mid": "45454B",
    "wither_shadow": "2A2A2F",
    "wither_line": "121214",
    "feather_vane": "F7F6F1",
    "feather_shade": "CFCDC4",
    "feather_quill": "BFAE86",
    "feather_line": "8B887E",
    "warm_vane": "F2D6AC",
    "warm_shade": "C99863",
    "warm_quill": "8E5E32",
    "warm_line": "6E4524",
    "cold_vane": "E9F0F7",
    "cold_shade": "A9BCD2",
    "cold_quill": "7F8FA6",
    "cold_line": "56627A",
    "emerald_light": "8DFFB0",
    "emerald_mid": "26CF5C",
    "emerald_dark": "0F8A35",
    "emerald_line": "06471A",
    # Slime is authored neutral and tinted green (or the player's colour) in game.
    "slime_hi": "FFFFFF",
    "slime_body": "D8D8D8",
    "slime_low": "A6A6A6",
    "slime_line": "6A6A6A",
    "glint_core": "FFFFFF",
    "glint_arm": "E6E6E6",
    "glint_edge": "A8A8A8",
    "ame_core": "FBF4FF",
    "ame_arm": "D7B8FF",
    "ame_edge": "9466E0",
    "charge_core": "F4FEFF",
    "charge_mid": "7FE3FF",
    "charge_edge": "2C7FD6",
    "soul_core": "D8FCFF",
    "soul_mid": "6FE3F2",
    "soul_edge": "2A97B4",
    "ring_hi": "FFFFFF",
    "ring_body": "DCE9F0",
}


def srgb_to_linear(c):
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def hex_rgb(h):
    return tuple(srgb_to_linear(int(h[i:i + 2], 16) / 255.0) for i in (0, 2, 4))


# ---------------------------------------------------------------- helpers
def fail(msg):
    raise SystemExit(f"render_assets: {msg}")


def args():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    out, only, smoke = None, None, False
    i = 0
    while i < len(argv):
        if argv[i] == "--out":
            out = argv[i + 1]
            i += 2
        elif argv[i] == "--only":
            only = set(argv[i + 1].split(","))
            i += 2
        elif argv[i] == "--smoke":
            smoke = True
            i += 1
        else:
            fail(f"unknown argument {argv[i]}")
    if not out:
        fail("--out <art folder> is required")
    return os.path.abspath(out), only, smoke


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def material(name, hex_color, alpha_keys=None):
    """A flat emission material; optional alpha keys [(frame, alpha)]."""
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    nt.nodes.clear()
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    em = nt.nodes.new("ShaderNodeEmission")
    em.inputs["Color"].default_value = (*hex_rgb(hex_color), 1.0)
    em.inputs["Strength"].default_value = 1.0
    if alpha_keys:
        tr = nt.nodes.new("ShaderNodeBsdfTransparent")
        mix = nt.nodes.new("ShaderNodeMixShader")
        nt.links.new(tr.outputs[0], mix.inputs[1])
        nt.links.new(em.outputs[0], mix.inputs[2])
        nt.links.new(mix.outputs[0], out.inputs["Surface"])
        for frame, a in alpha_keys:
            mix.inputs[0].default_value = a
            mix.inputs[0].keyframe_insert("default_value", frame=frame)
        m.surface_render_method = "BLENDED"
    else:
        nt.links.new(em.outputs[0], out.inputs["Surface"])
        m.surface_render_method = "DITHERED"
    return m


def mesh_object(name, coll, verts, faces, mats, face_mats, z=0.0):
    me = bpy.data.meshes.new(name)
    me.from_pydata([(x, y, z) for x, y in verts], [], faces)
    me.update()
    for m in mats:
        me.materials.append(m)
    for poly, mi in zip(me.polygons, face_mats):
        poly.material_index = mi
    ob = bpy.data.objects.new(name, me)
    coll.objects.link(ob)
    return ob


def outline_poly(points, grow):
    """A polygon pushed outward by `grow` along its vertex normals (for a
    darker outline face behind the body)."""
    n = len(points)
    out = []
    cx = sum(p[0] for p in points) / n
    cy = sum(p[1] for p in points) / n
    for x, y in points:
        dx, dy = x - cx, y - cy
        d = math.hypot(dx, dy) or 1.0
        out.append((x + dx / d * grow, y + dy / d * grow))
    return out


def poly_object(name, coll, layers):
    """Stacked flat polygons: [(points, material, z)] -> one object."""
    verts, faces, face_mats, mats = [], [], [], []
    for points, mat, z in layers:
        base = len(verts)
        verts.extend((x, y, z) for x, y in points)
        faces.append(list(range(base, base + len(points))))
        if mat not in mats:
            mats.append(mat)
        face_mats.append(mats.index(mat))
    me = bpy.data.meshes.new(name)
    me.from_pydata(verts, [], faces)
    me.update()
    for m in mats:
        me.materials.append(m)
    for poly, mi in zip(me.polygons, face_mats):
        poly.material_index = mi
    ob = bpy.data.objects.new(name, me)
    coll.objects.link(ob)
    return ob


def strip(points, width):
    """A flat strip of `width` along a polyline, as one polygon outline."""
    left, right = [], []
    for i, (x, y) in enumerate(points):
        a = points[max(i - 1, 0)]
        b = points[min(i + 1, len(points) - 1)]
        tx, ty = b[0] - a[0], b[1] - a[1]
        d = math.hypot(tx, ty) or 1.0
        nx, ny = -ty / d * width / 2, tx / d * width / 2
        left.append((x + nx, y + ny))
        right.append((x - nx, y - ny))
    return left + right[::-1]


def key_scale(ob, keys):
    for frame, s in keys:
        ob.scale = (s[0], s[1], 1.0) if isinstance(s, tuple) else (s, s, 1.0)
        ob.keyframe_insert("scale", frame=frame)
    set_constant(ob)


def key_hide(ob, frames_visible, last):
    for f in range(1, last + 1):
        ob.hide_render = f not in frames_visible
        ob.keyframe_insert("hide_render", frame=f)
    set_constant(ob)


def set_constant(ob):
    """Stepped keys: each frame is an authored pose, no in-between drift."""
    ad = ob.animation_data
    if not ad or not ad.action:
        return
    for fc in fcurves(ad.action):
        for k in fc.keyframe_points:
            k.interpolation = "CONSTANT"


def fcurves(action):
    if hasattr(action, "fcurves"):
        return list(action.fcurves)
    out = []
    for layer in action.layers:
        for strip_ in layer.strips:
            for bag in strip_.channelbags:
                out.extend(bag.fcurves)
    return out


# ---------------------------------------------------------------- builders
# Each builder authors one animation in its own collection, returns
# (frame_count, durations_ms, cell_px). Units: the camera shows a 1x1 area
# centred on the origin, so one pixel is 1/cell units.


def build_shard(coll, pal, variant):
    L, M, S, O = (material(f"{coll.name}_{k}", PALETTE[f"{pal}_{k}"]) for k in ("light", "mid", "shadow", "line"))
    if variant == 0:
        body = [(-0.30, -0.10), (-0.05, 0.30), (0.28, 0.18), (0.22, -0.22), (-0.10, -0.30)]
        light = [(-0.30, -0.10), (-0.05, 0.30), (0.02, 0.02), (-0.12, -0.14)]
        shadow = [(0.02, 0.02), (0.28, 0.18), (0.22, -0.22)]
    else:
        body = [(-0.34, 0.05), (-0.12, 0.26), (0.30, 0.10), (0.18, -0.12), (-0.06, -0.26)]
        light = [(-0.34, 0.05), (-0.12, 0.26), (0.06, 0.12), (-0.10, -0.04)]
        shadow = [(0.06, 0.12), (0.30, 0.10), (0.18, -0.12), (0.00, -0.08)]
    poly_object(coll.name, coll, [
        (outline_poly(body, 0.06), O, 0.0),
        (body, M, 0.01),
        (light, L, 0.02),
        (shadow, S, 0.02),
    ])
    return 1, [100], 16


def build_feather(coll, pal):
    V, S, Q, O = (material(f"{coll.name}_{k}", PALETTE[f"{pal}_{k}"]) for k in ("vane", "shade", "quill", "line"))
    # A tapered vane with three notches (barb groups), on a diagonal quill.
    vane = [(-0.34, -0.30), (-0.20, -0.06), (-0.22, 0.04), (-0.06, 0.16), (-0.08, 0.24),
            (0.12, 0.36), (0.30, 0.34), (0.26, 0.14), (0.12, 0.06), (0.16, -0.04),
            (0.02, -0.10), (0.04, -0.20), (-0.18, -0.28)]
    shade = [(-0.30, -0.27), (0.04, -0.20), (0.02, -0.10), (0.16, -0.04), (0.12, 0.06),
             (0.26, 0.14), (0.30, 0.34), (0.20, 0.20), (0.0, 0.0), (-0.20, -0.18)]
    quill = strip([(-0.40, -0.40), (-0.12, -0.12), (0.12, 0.12), (0.30, 0.34)], 0.07)
    poly_object(coll.name, coll, [
        (outline_poly(vane, 0.06), O, 0.0),
        (vane, V, 0.01),
        (shade, S, 0.02),
        (quill, Q, 0.03),
    ])
    return 1, [100], 16


def build_emerald(coll):
    Lm, Mm, Dm, Om = (material(f"{coll.name}_{k}", PALETTE[f"emerald_{k}"]) for k in ("light", "mid", "dark", "line"))
    # A tall hexagonal chip: dark left facets, bright upper right, one glint.
    body = [(0.0, 0.34), (0.20, 0.17), (0.20, -0.17), (0.0, -0.34), (-0.20, -0.17), (-0.20, 0.17)]
    left = [(-0.20, 0.17), (0.0, 0.08), (0.0, -0.34), (-0.20, -0.17)]
    top = [(0.0, 0.34), (0.20, 0.17), (0.0, 0.08), (-0.20, 0.17)]
    hi = [(0.04, 0.24), (0.10, 0.20), (0.06, 0.14)]
    poly_object(coll.name, coll, [
        (outline_poly(body, 0.07), Om, 0.0),
        (body, Mm, 0.01),
        (left, Dm, 0.02),
        (top, Lm, 0.02),
        (hi, material(f"{coll.name}_spark", "FFFFFF"), 0.03),
    ])
    return 1, [100], 16


def blob_points(rx, ry, n=10, flat=0.25):
    pts = []
    for i in range(n):
        a = math.tau * i / n - math.pi / 2
        x, y = math.cos(a) * rx, math.sin(a) * ry
        if y < 0:
            y *= 1.0 - flat  # flatter underside
        pts.append((x, y))
    return pts


def build_slime(coll, radius, cell):
    H, B, Lw, O = (material(f"{coll.name}_{k}", PALETTE[f"slime_{k}"]) for k in ("hi", "body", "low", "line"))
    body = blob_points(radius, radius * 0.82)
    low = [(x, y) for x, y in blob_points(radius * 0.92, radius * 0.70) if y <= 0.0]
    low = low + [(-low[0][0], low[0][1])] if low else low
    hi = blob_points(radius * 0.26, radius * 0.18, n=6)
    hi = [(x - radius * 0.32, y + radius * 0.36) for x, y in hi]
    ob = poly_object(coll.name, coll, [
        (outline_poly(body, 1.6 / cell), O, 0.0),
        (body, B, 0.01),
        ([(x, y - radius * 0.08) for x, y in low], Lw, 0.02),
        (hi, H, 0.03),
    ])
    # Squash and stretch about a fixed centre: neutral, stretch, compact,
    # settle, shrink, last drop.
    key_scale(ob, [(1, 1.0), (2, (0.82, 1.22)), (3, (1.18, 0.80)), (4, (1.04, 0.96)),
                   (5, (0.78, 0.74)), (6, (0.50, 0.46))])
    return 6, [60, 50, 50, 60, 70, 80], cell


def build_glint(coll, core, arm, edge):
    C, A, E = (material(f"{coll.name}_{k}", PALETTE[k2]) for k, k2 in (("core", core), ("arm", arm), ("edge", edge)))
    # A tall four-point star (two crossed diamonds) and a small core square.
    def star(lv, lh, w):
        v = [(0.0, lv), (w, 0.0), (0.0, -lv), (-w, 0.0)]
        h = [(lh, 0.0), (0.0, w), (-lh, 0.0), (0.0, -w)]
        return v, h
    parts = []
    # frame: (vertical arm, horizontal arm, arm width, core)
    poses = [(0.10, 0.08, 0.05, 0.05), (0.22, 0.16, 0.06, 0.06), (0.40, 0.28, 0.07, 0.08),
             (0.30, 0.22, 0.06, 0.07), (0.16, 0.12, 0.05, 0.05), (0.07, 0.07, 0.04, 0.04)]
    for i, (lv, lh, w, c) in enumerate(poses, start=1):
        v, h = star(lv, lh, w)
        ob = poly_object(f"{coll.name}_pose{i}", coll, [
            (outline_poly(v, 0.05), E, 0.0),
            (outline_poly(h, 0.05), E, 0.0),
            (v, A, 0.01),
            (h, A, 0.01),
            ([(-c, -c), (c, -c), (c, c), (-c, c)], C, 0.02),
        ])
        key_hide(ob, {i}, len(poses))
        parts.append(ob)
    return len(poses), [30, 40, 90, 70, 50, 30], 16


def build_charge(coll, seed):
    Cc, Cm, Ce = (material(f"{coll.name}_{k}", PALETTE[f"charge_{k}"]) for k in ("core", "mid", "edge"))
    rnd = __import__("random").Random(seed)
    # One strong fork with one small secondary branch.
    main = [(-0.36, 0.30)]
    for _ in range(5):
        x, y = main[-1]
        main.append((x + 0.13 + rnd.uniform(-0.02, 0.03), y - 0.12 + rnd.uniform(-0.08, 0.08)))
    fork_at = main[2]
    branch = [fork_at, (fork_at[0] + 0.06, fork_at[1] - 0.18), (fork_at[0] + 0.02, fork_at[1] - 0.30)]
    frames = [
        (main[:3], None, 1.0),   # onset: half the bolt
        (main, None, 1.0),       # full bolt
        (main, branch, 1.0),     # bolt and fork
        (main[1:], branch, 0.7),  # breaking up
    ]
    for i, (m, b, w) in enumerate(frames, start=1):
        layers = [(strip(m, 0.14 * w), Ce, 0.0), (strip(m, 0.08 * w), Cm, 0.01), (strip(m, 0.035 * w), Cc, 0.02)]
        if b:
            layers += [(strip(b, 0.10 * w), Ce, 0.0), (strip(b, 0.05 * w), Cm, 0.01)]
        ob = poly_object(f"{coll.name}_pose{i}", coll, layers)
        key_hide(ob, {i}, len(frames))
    return len(frames), [60, 120, 120, 100], 32


def wisp(height, width, curl, wobble, steps=9):
    """A teardrop plume: a round base tapering up a curled spine."""
    spine = []
    for k in range(steps):
        u = k / (steps - 1)
        x = curl * u * u + wobble * math.sin(u * math.pi * 2.0)
        spine.append((x, -0.30 + height * u))
    left, right = [], []
    for k, (x, y) in enumerate(spine):
        u = k / (steps - 1)
        w = width * min(1.0, 0.55 + u * 3.0) * (1.0 - u) ** 0.9
        left.append((x - w, y))
        right.append((x + w, y))
    bottom = (spine[0][0], spine[0][1] - width * 0.45)
    return [bottom] + right + left[::-1][1:-1]


def build_soul(coll, sprint):
    Sc, Sm, Se = (material(f"{coll.name}_{k}", PALETTE[f"soul_{k}"]) for k in ("core", "mid", "edge"))
    n = 5 if sprint else 8
    for i in range(1, n + 1):
        t = (i - 1) / (n - 1)
        grow = min(1.0, 0.45 + t * 1.4)
        height = (0.56 if sprint else 0.68) * grow
        width = (0.17 if sprint else 0.15) * (1.0 - 0.45 * t)
        curl = (0.10 if sprint else -0.14) * math.sin(t * math.pi * 1.2)
        wobble = 0.03 * math.sin(t * 5.0)
        layers = []
        if t < 0.88:
            body = wisp(height, width, curl, wobble)
            core = wisp(height * 0.62, width * 0.45, curl * 0.6, wobble * 0.6)
            layers += [(outline_poly(body, 0.05), Se, 0.0), (body, Sm, 0.01), (core, Sc, 0.02)]
        # Detached motes drift up as the plume thins.
        for j, (mx, my) in enumerate([(0.17, -0.06), (-0.16, 0.10)]):
            if t > 0.35 + 0.2 * j:
                r = 0.045 if t < 0.9 else 0.032
                y = my + 0.28 * t
                layers.append(([(mx - r, y - r), (mx + r, y - r), (mx + r, y + r), (mx - r, y + r)], Sm, 0.01))
        ob = poly_object(f"{coll.name}_pose{i}", coll, layers)
        key_hide(ob, {i}, n)
    durations = [60, 60, 70, 80, 90] if sprint else [80, 90, 100, 110, 120, 120, 120, 140]
    return n, durations, 32


def build_ring(coll):
    Hi, Bd = (material(f"{coll.name}_{k}", PALETTE[f"ring_{k}"]) for k in ("hi", "body"))
    n = 6
    seg = 16
    for i in range(1, n + 1):
        t = (i - 1) / (n - 1)
        r = 0.14 + 0.32 * t
        th = (0.10 - 0.05 * t)
        outer = [(math.cos(math.tau * k / seg) * r, math.sin(math.tau * k / seg) * r) for k in range(seg)]
        inner = [(math.cos(math.tau * k / seg) * (r - th), math.sin(math.tau * k / seg) * (r - th)) for k in range(seg)]
        verts = outer + inner
        faces = [[k, (k + 1) % seg, seg + (k + 1) % seg, seg + k] for k in range(seg)]
        mats = [Bd, Hi]
        # The upper-left quarter catches the light.
        face_mats = [1 if 4 <= k <= 7 else 0 for k in range(seg)]
        ob = mesh_object(f"{coll.name}_pose{i}", coll, verts, faces, mats, face_mats)
        key_hide(ob, {i}, n)
    # The fade-out over the last frames is the runtime's alpha curve.
    return n, [40, 40, 50, 50, 60, 70], 32


# animation id -> (family .blend, builder)
ANIMATIONS = [
    ("bone_a", "hit_fragments", lambda c: build_shard(c, "bone", 0)),
    ("bone_b", "hit_fragments", lambda c: build_shard(c, "bone", 1)),
    ("wither_bone_a", "hit_fragments", lambda c: build_shard(c, "wither", 0)),
    ("wither_bone_b", "hit_fragments", lambda c: build_shard(c, "wither", 1)),
    ("feather", "hit_fragments", lambda c: build_feather(c, "feather")),
    ("feather_warm", "hit_fragments", lambda c: build_feather(c, "warm")),
    ("feather_cold", "hit_fragments", lambda c: build_feather(c, "cold")),
    ("emerald", "hit_fragments", build_emerald),
    ("slime_small", "slime", lambda c: build_slime(c, 0.22, 16)),
    ("slime_medium", "slime", lambda c: build_slime(c, 0.28, 16)),
    ("slime_large", "slime", lambda c: build_slime(c, 0.32, 32)),
    ("sparkle", "glints", lambda c: build_glint(c, "glint_core", "glint_arm", "glint_edge")),
    ("sparkle_amethyst", "glints", lambda c: build_glint(c, "ame_core", "ame_arm", "ame_edge")),
    ("charge_a", "charge", lambda c: build_charge(c, 11)),
    ("charge_b", "charge", lambda c: build_charge(c, 23)),
    ("charge_c", "charge", lambda c: build_charge(c, 37)),
    ("soul_ambient", "soul", lambda c: build_soul(c, False)),
    ("soul_sprint", "soul", lambda c: build_soul(c, True)),
    ("water_ring", "water", build_ring),
]


# ---------------------------------------------------------------- stage
def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def stage(scene, cell):
    r = scene.render
    engines = [i.identifier for i in bpy.types.RenderSettings.bl_rna.properties["engine"].enum_items]
    if "BLENDER_EEVEE" not in engines:
        fail(f"BLENDER_EEVEE not available ({engines})")
    r.engine = "BLENDER_EEVEE"
    scene.eevee.taa_render_samples = 1
    r.resolution_x = cell
    r.resolution_y = cell
    r.resolution_percentage = 100
    r.pixel_aspect_x = 1.0
    r.pixel_aspect_y = 1.0
    r.film_transparent = True
    r.use_motion_blur = False
    r.use_compositing = False
    r.use_sequencer = False
    # One sample gives binary coverage; the narrowest filter keeps it so.
    fp = bpy.types.RenderSettings.bl_rna.properties["filter_size"]
    r.filter_size = max(fp.hard_min, 0.01)
    r.image_settings.file_format = "PNG"
    r.image_settings.color_mode = "RGBA"
    r.image_settings.color_depth = "8"
    r.image_settings.compression = 100
    vs = scene.view_settings
    scene.display_settings.display_device = "sRGB"
    vs.view_transform = "Standard"
    vs.look = "None"
    vs.exposure = 0.0
    vs.gamma = 1.0
    if vs.view_transform != "Standard":
        fail("Standard view transform unavailable")
    if "STAGE" not in bpy.data.collections:
        st = bpy.data.collections.new("STAGE")
        scene.collection.children.link(st)
        cam_data = bpy.data.cameras.new("PixelCamera")
        cam_data.type = "ORTHO"
        cam_data.ortho_scale = 1.0
        cam_data.clip_start = 0.1
        cam_data.clip_end = 10.0
        cam_data.shift_x = 0.0
        cam_data.shift_y = 0.0
        cam = bpy.data.objects.new("PixelCamera", cam_data)
        cam.location = (0.0, 0.0, 5.0)
        cam.rotation_euler = (0.0, 0.0, 0.0)
        st.objects.link(cam)
        scene.camera = cam


def main():
    out, only, smoke = args()
    if tuple(bpy.app.version) != PINNED_VERSION:
        fail(f"Blender {bpy.app.version_string} found, {PINNED_VERSION} pinned")
    script_hash = sha256_file(os.path.abspath(__file__))
    blend_dir = os.path.join(out, "blender")
    render_dir = os.path.join(out, "renders")
    os.makedirs(blend_dir, exist_ok=True)
    provenance_path = os.path.join(out, "manifests", "render_provenance.json")
    os.makedirs(os.path.dirname(provenance_path), exist_ok=True)
    records = {}
    if os.path.exists(provenance_path) and only:
        with open(provenance_path, encoding="utf-8") as f:
            records = {a["id"]: a for a in json.load(f)["animations"]}

    families = []
    for _, fam, _ in ANIMATIONS:
        if fam not in families:
            families.append(fam)
    for fam in families:
        todo = [a for a in ANIMATIONS if a[1] == fam and (not only or a[0] in only)]
        if not todo:
            continue
        reset()
        scene = bpy.context.scene
        scene.name = fam
        built = []
        for anim_id, _, builder in todo:
            coll = bpy.data.collections.new(anim_id)
            scene.collection.children.link(coll)
            frames, durations, cell = builder(coll)
            if len(durations) != frames:
                fail(f"{anim_id}: {frames} frames but {len(durations)} durations")
            built.append((anim_id, coll, frames, durations, cell))
        blend_path = os.path.join(blend_dir, f"{fam}.blend")
        for anim_id, coll, frames, durations, cell in built:
            stage(scene, cell)
            for other in scene.collection.children:
                if other.name not in ("STAGE",):
                    other.hide_render = other is not coll
            scene.frame_start = 1
            scene.frame_end = frames
            frame_records = []
            folder = os.path.join(render_dir, anim_id)
            os.makedirs(folder, exist_ok=True)
            for old in os.listdir(folder):
                os.remove(os.path.join(folder, old))
            for f in range(1, (1 if smoke else frames) + 1):
                scene.frame_set(f)
                path = os.path.join(folder, f"{f:02d}.png")
                scene.render.filepath = path
                bpy.ops.render.render(write_still=True)
                if not os.path.exists(path):
                    fail(f"{anim_id} frame {f}: no output")
                frame_records.append({
                    "frame": f,
                    "file": os.path.relpath(path, out).replace(os.sep, "/"),
                    "sha256": sha256_file(path),
                    "duration_ms": durations[f - 1],
                })
            records[anim_id] = {
                "id": anim_id,
                "blend": f"blender/{fam}.blend",
                "collection": anim_id,
                "camera": "PixelCamera",
                "frames": frame_records,
                "cell": [cell, cell],
                "settings": {
                    "engine": scene.render.engine,
                    "samples": scene.eevee.taa_render_samples,
                    "filter_size": round(scene.render.filter_size, 4),
                    "film_transparent": scene.render.film_transparent,
                    "view_transform": scene.view_settings.view_transform,
                    "look": scene.view_settings.look,
                    "ortho_scale": scene.camera.data.ortho_scale,
                    "png": "RGBA8",
                },
            }
        for other in scene.collection.children:
            other.hide_render = False
        bpy.ops.wm.save_as_mainfile(filepath=blend_path, compress=True)
        for anim_id, *_ in built:
            records[anim_id]["blend_sha256"] = sha256_file(blend_path)
    provenance = {
        "blender": {"version": bpy.app.version_string, "build_hash": bpy.app.build_hash.decode()},
        "script": "blender/render_assets.py",
        "script_sha256": script_hash,
        "palette": PALETTE,
        "animations": [records[a[0]] for a in ANIMATIONS if a[0] in records],
    }
    with open(provenance_path, "w", encoding="utf-8", newline="\n") as f:
        json.dump(provenance, f, indent=1, sort_keys=False)
        f.write("\n")
    print(f"render_assets: {len(provenance['animations'])} animations rendered into {render_dir}")


def render_icon(out):
    """The project icon: a dark tile with a feather, a slime blob, a soul
    wisp and a sparkle, built from the same authored meshes, at 128 px."""
    reset()
    scene = bpy.context.scene
    scene.name = "icon"
    stage(scene, 128)
    tile = bpy.data.collections.new("TILE")
    scene.collection.children.link(tile)
    r = 0.47
    corner = 0.08
    pts = []
    for cx, cy, a0 in ((r - corner, r - corner, 0), (-r + corner, r - corner, 90),
                       (-r + corner, -r + corner, 180), (r - corner, -r + corner, 270)):
        for k in range(5):
            a = math.radians(a0 + k * 22.5)
            pts.append((cx + math.cos(a) * corner, cy + math.sin(a) * corner))
    poly_object("Tile", tile, [
        (pts, material("tile_edge", "2A2340"), -0.2),
        ([(x * 0.94, y * 0.94) for x, y in pts], material("tile_body", "3B3159"), -0.19),
    ])
    parts = [
        ("feather", lambda c: build_feather(c, "feather"), (-0.17, 0.13), 0.56, 1),
        ("slime", lambda c: build_slime(c, 0.32, 32), (0.15, -0.20), 0.62, 1),
        ("soul", lambda c: build_soul(c, False), (0.20, 0.17), 0.48, 4),
        ("sparkle", lambda c: build_glint(c, "ame_core", "ame_arm", "ame_edge"), (-0.20, -0.20), 0.62, 3),
    ]
    for name, builder, (x, y), scale, frame in parts:
        coll = bpy.data.collections.new(name)
        scene.collection.children.link(coll)
        builder(coll)
        if name == "slime":
            # In game the neutral slime is tinted by the default colour; the
            # icon shows that same multiply on its materials.
            tint = hex_rgb("88FF79")
            for ob in coll.objects:
                for m in ob.data.materials:
                    em = m.node_tree.nodes.get("Emission")
                    c = em.inputs["Color"].default_value
                    em.inputs["Color"].default_value = (c[0] * tint[0], c[1] * tint[1], c[2] * tint[2], 1.0)
        for ob in coll.objects:
            ad = ob.animation_data
            if ad and ad.action:
                scene.frame_set(frame)
                hidden = ob.hide_render
                scale_now = tuple(ob.scale)
                ob.animation_data_clear()
                ob.hide_render = hidden
                ob.scale = scale_now
            ob.location = (x, y, ob.location.z)
            ob.scale = (ob.scale[0] * scale, ob.scale[1] * scale, 1.0)
    scene.frame_set(1)
    os.makedirs(os.path.join(out, "icon"), exist_ok=True)
    path = os.path.join(out, "icon", "visuality_horizon.png")
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True)
    bpy.ops.wm.save_as_mainfile(filepath=os.path.join(out, "blender", "icon.blend"), compress=True)
    print(f"render_assets: icon {path} {sha256_file(path)}")


if '--icon' in sys.argv:
    sys.argv.remove('--icon')
    render_icon(args()[0])
else:
    main()
