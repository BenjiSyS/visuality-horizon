"""Minimal lossless PNG reading and writing (RGBA8), standard library only.

Used to copy Blender-rendered rectangles into the atlas and to lay out
review sheets. It never draws artwork: pixels are copied or, for review
sheets only, composited over flat backgrounds and enlarged by repetition.
"""

import struct
import zlib

SIG = b"\x89PNG\r\n\x1a\n"
MAX_SIDE = 8192


def read(path):
    """Returns (width, height, bytearray RGBA8)."""
    with open(path, "rb") as f:
        data = f.read()
    if data[:8] != SIG:
        raise ValueError(f"{path}: not a PNG")
    pos = 8
    ihdr = None
    idat = []
    palette = None
    trns = None
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos:pos + 4])
        kind = data[pos + 4:pos + 8]
        body = data[pos + 8:pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            ihdr = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"PLTE":
            palette = body
        elif kind == b"tRNS":
            trns = body
        elif kind == b"IEND":
            break
    if not ihdr:
        raise ValueError(f"{path}: no IHDR")
    w, h, depth, ctype, _, _, interlace = ihdr
    if not (0 < w <= MAX_SIDE and 0 < h <= MAX_SIDE):
        raise ValueError(f"{path}: size {w}x{h} out of range")
    if depth != 8 or interlace != 0:
        raise ValueError(f"{path}: only 8-bit non-interlaced PNG is supported")
    channels = {6: 4, 2: 3, 0: 1, 4: 2, 3: 1}.get(ctype)
    if channels is None:
        raise ValueError(f"{path}: colour type {ctype} unsupported")
    raw = zlib.decompress(b"".join(idat))
    stride = w * channels
    out = bytearray(w * h * 4)
    prev = bytearray(stride)
    i = 0
    for y in range(h):
        ft = raw[i]
        line = bytearray(raw[i + 1:i + 1 + stride])
        i += 1 + stride
        bpp = channels
        for x in range(stride):
            a = line[x - bpp] if x >= bpp else 0
            b = prev[x]
            c = prev[x - bpp] if x >= bpp else 0
            if ft == 1:
                line[x] = (line[x] + a) & 255
            elif ft == 2:
                line[x] = (line[x] + b) & 255
            elif ft == 3:
                line[x] = (line[x] + ((a + b) >> 1)) & 255
            elif ft == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pr = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                line[x] = (line[x] + pr) & 255
            elif ft != 0:
                raise ValueError(f"{path}: bad filter {ft}")
        prev = line
        for x in range(w):
            o = (y * w + x) * 4
            if ctype == 6:
                out[o:o + 4] = line[x * 4:x * 4 + 4]
            elif ctype == 2:
                out[o:o + 3] = line[x * 3:x * 3 + 3]
                out[o + 3] = 255
            elif ctype == 0:
                out[o:o + 3] = bytes([line[x]] * 3)
                out[o + 3] = 255
            elif ctype == 4:
                out[o:o + 3] = bytes([line[x * 2]] * 3)
                out[o + 3] = line[x * 2 + 1]
            else:
                idx = line[x]
                out[o:o + 3] = palette[idx * 3:idx * 3 + 3]
                out[o + 3] = trns[idx] if trns and idx < len(trns) else 255
    return w, h, out


def write(path, w, h, rgba):
    """Writes RGBA8 deterministically (no timestamps, fixed compression)."""
    raw = bytearray()
    for y in range(h):
        raw.append(0)
        raw.extend(rgba[y * w * 4:(y + 1) * w * 4])

    def chunk(kind, body):
        c = struct.pack(">I", len(body)) + kind + body
        return c + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)

    png = SIG + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    png += chunk(b"sRGB", b"\x00")
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)
