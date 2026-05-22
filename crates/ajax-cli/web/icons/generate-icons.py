#!/usr/bin/env python3
"""Generate the Ajax PWA app icons.

Pure-stdlib PNG encoder so the repo needs no image dependency. Re-run after
changing the mark; commit the resulting PNGs as binary assets.
"""
import struct
import zlib
from pathlib import Path

DARK = (16, 20, 26)      # #10141a  background
TEAL = (45, 212, 191)    # #2dd4bf  brand mark

# Glyph "A" geometry in unit (0..1) coordinates.
APEX = (0.5, 0.22)
LEFT = (0.27, 0.78)
RIGHT = (0.73, 0.78)
STROKE_HW = 0.072        # half stroke width
CROSSBAR_Y = 0.56
CORNER_R = 0.22          # rounded-square radius (non-maskable icons)


def seg_d2(px, py, ax, ay, bx, by):
    dx, dy = bx - ax, by - ay
    length2 = dx * dx + dy * dy
    t = ((px - ax) * dx + (py - ay) * dy) / length2
    t = 0.0 if t < 0.0 else 1.0 if t > 1.0 else t
    cx, cy = ax + t * dx, ay + t * dy
    return (px - cx) ** 2 + (py - cy) ** 2


def crossbar_endpoints():
    def x_at(leg_end):
        t = (CROSSBAR_Y - APEX[1]) / (leg_end[1] - APEX[1])
        return APEX[0] + t * (leg_end[0] - APEX[0])
    return (x_at(LEFT), CROSSBAR_Y), (x_at(RIGHT), CROSSBAR_Y)


def in_glyph(u, v):
    hw2 = STROKE_HW ** 2
    cb_a, cb_b = crossbar_endpoints()
    return (
        seg_d2(u, v, *APEX, *LEFT) <= hw2
        or seg_d2(u, v, *APEX, *RIGHT) <= hw2
        or seg_d2(u, v, *cb_a, *cb_b) <= hw2
    )


def in_rounded_square(u, v, r):
    if r <= 0:
        return True
    lo, hi = r, 1.0 - r
    cx = lo if u < lo else hi if u > hi else u
    cy = lo if v < lo else hi if v > hi else v
    return (u - cx) ** 2 + (v - cy) ** 2 <= r * r


def render(size, opaque):
    """Render an RGBA icon. opaque=True fills the full square (maskable)."""
    ss = 3
    radius = 0.0 if opaque else CORNER_R
    pixels = bytearray()
    for y in range(size):
        pixels.append(0)  # PNG filter byte: none
        for x in range(size):
            r_acc = g_acc = b_acc = a_acc = 0
            for sy in range(ss):
                for sx in range(ss):
                    u = (x + (sx + 0.5) / ss) / size
                    v = (y + (sy + 0.5) / ss) / size
                    if in_glyph(u, v):
                        r_acc += TEAL[0]; g_acc += TEAL[1]; b_acc += TEAL[2]; a_acc += 255
                    elif in_rounded_square(u, v, radius):
                        r_acc += DARK[0]; g_acc += DARK[1]; b_acc += DARK[2]; a_acc += 255
            n = ss * ss
            pixels += bytes((r_acc // n, g_acc // n, b_acc // n, a_acc // n))
    return write_png(size, size, pixels)


def write_png(width, height, raw):
    def chunk(tag, data):
        return (struct.pack(">I", len(data)) + tag + data
                + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF))
    sig = b"\x89PNG\r\n\x1a\n"
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    idat = zlib.compress(bytes(raw), 9)
    return sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")


def main():
    out = Path(__file__).parent
    targets = [
        ("icon-192.png", 192, False),
        ("icon-512.png", 512, False),
        ("icon-maskable-512.png", 512, True),
        ("apple-touch-icon.png", 180, True),
    ]
    for name, size, opaque in targets:
        (out / name).write_bytes(render(size, opaque))
        print(f"wrote {name} ({size}x{size})")


if __name__ == "__main__":
    main()
