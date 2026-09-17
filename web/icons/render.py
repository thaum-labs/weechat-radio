#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Rasterize the WeeChat Radio mark into PNG app icons (stdlib only)."""

from __future__ import annotations

import math
import struct
import zlib
from pathlib import Path

CHEVRON = (216, 208, 232)
ACCENT = (125, 155, 255)
BG = (7, 7, 10)
WAVE = {
    "accent": ACCENT,
    "internet": (0, 229, 255),
    "internet-radio": (57, 255, 20),
    "radio": (255, 191, 0),
    "radio-plus": (255, 77, 255),
}


def dist_seg(px: float, py: float, ax: float, ay: float, bx: float, by: float) -> float:
    vx, vy = bx - ax, by - ay
    l2 = vx * vx + vy * vy
    if l2 < 1e-12:
        return math.hypot(px - ax, py - ay)
    t = max(0.0, min(1.0, ((px - ax) * vx + (py - ay) * vy) / l2))
    return math.hypot(px - (ax + t * vx), py - (ay + t * vy))


def dist_arc(px: float, py: float, cx: float, cy: float, radius: float) -> float:
    dx, dy = px - cx, py - cy
    ang = math.atan2(dy, dx)
    if -math.pi / 2.0 <= ang <= 0.0:
        return abs(math.hypot(dx, dy) - radius)
    e0 = math.hypot(px - cx, py - (cy - radius))
    e1 = math.hypot(px - (cx + radius), py - cy)
    return min(e0, e1)


def rbox(px: float, py: float, cx: float, cy: float, hw: float, hh: float, rad: float) -> float:
    dx = abs(px - cx) - (hw - rad)
    dy = abs(py - cy) - (hh - rad)
    ox, oy = max(dx, 0.0), max(dy, 0.0)
    return math.hypot(ox, oy) + min(max(dx, dy), 0.0) - rad


def cover(signed: float, aa: float) -> float:
    return max(0.0, min(1.0, 0.5 - signed / aa))


def over(dst: list[int], r: int, g: int, b: int, a: float) -> None:
    a = max(0.0, min(1.0, a))
    if a <= 0.0:
        return
    inv = 1.0 - a
    dst[0] = round(r * a + dst[0] * inv)
    dst[1] = round(g * a + dst[1] * inv)
    dst[2] = round(b * a + dst[2] * inv)
    dst[3] = round(255.0 * (a + (dst[3] / 255.0) * inv))


def raster(size: int, wave: tuple[int, int, int], framed: bool, round_tile: bool) -> bytes:
    n = size
    out = bytearray(n * n * 4)
    dim = float(size)
    pad = dim * (0.18 if framed else 0.08)
    inner = dim - 2.0 * pad
    scale = min(inner / 64.0, inner / 60.0)
    ox = (dim - 64.0 * scale) * 0.5
    oy = (dim - 60.0 * scale) * 0.5
    aa = 0.65
    radius = dim * (26.0 / 120.0) if round_tile else 0.0
    border_w = max(1.0, dim / 120.0)
    for y in range(n):
        for x in range(n):
            px = x + 0.5
            py = y + 0.5
            pix = [0, 0, 0, 0]
            if framed:
                sdf = rbox(px, py, dim * 0.5, dim * 0.5, dim * 0.5 - 0.5, dim * 0.5 - 0.5, radius)
                over(pix, *BG, cover(sdf, aa) if round_tile else 1.0)
                if round_tile:
                    over(pix, *ACCENT, cover(abs(sdf) - border_w * 0.5, aa) * 0.22)
                else:
                    pix[0], pix[1], pix[2], pix[3] = BG[0], BG[1], BG[2], 255
            mx = (px - ox) / scale
            my = (py - oy) / scale
            aa_v = aa / scale
            d_chev = min(
                dist_seg(mx, my, 8.0, 20.0, 20.0, 30.0),
                dist_seg(mx, my, 20.0, 30.0, 8.0, 40.0),
            )
            over(pix, *CHEVRON, cover(d_chev - 2.5, aa_v))
            d_bar = rbox(mx, my, 32.5, 38.5, 7.5, 2.5, 1.0)
            over(pix, *wave, cover(d_bar, aa_v))
            wr, wg, wb = wave
            for radius_arc, op in ((7.0, 1.0), (13.0, 0.7), (19.0, 0.4)):
                d = dist_arc(mx, my, 40.0, 36.0, radius_arc)
                over(pix, wr, wg, wb, cover(d - 2.0, aa_v) * op)
            i = (y * n + x) * 4
            out[i : i + 4] = bytes(pix)
    return bytes(out)


def encode_png(w: int, h: int, rgba: bytes) -> bytes:
    def chunk(tag: bytes, data: bytes) -> bytes:
        crc = zlib.crc32(tag + data) & 0xFFFFFFFF
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", crc)

    raw = b"".join(b"\x00" + rgba[y * w * 4 : (y + 1) * w * 4] for y in range(h))
    ihdr = struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def write_png(path: Path, w: int, h: int, rgba: bytes) -> None:
    path.write_bytes(encode_png(w, h, rgba))


def write_ico(path: Path, sizes: list[int], wave: tuple[int, int, int]) -> None:
    images = [(n, encode_png(n, n, raster(n, wave, True, True))) for n in sizes]
    offset = 6 + 16 * len(images)
    out = bytearray(struct.pack("<HHH", 0, 1, len(images)))
    blobs = bytearray()
    for n, png in images:
        out += struct.pack(
            "<BBBBHHII",
            0 if n >= 256 else n,
            0 if n >= 256 else n,
            0,
            0,
            1,
            32,
            len(png),
            offset,
        )
        blobs += png
        offset += len(png)
    path.write_bytes(bytes(out) + bytes(blobs))


def main() -> None:
    root = Path(__file__).resolve().parents[1]
    repo = root.parent
    accent = WAVE["accent"]
    write_png(root / "favicon.png", 32, 32, raster(32, accent, True, True))
    write_png(root / "apple-touch-icon.png", 180, 180, raster(180, accent, True, False))
    write_png(root / "icon.png", 512, 512, raster(512, accent, True, True))
    icons = Path(__file__).resolve().parent
    for name, rgb in WAVE.items():
        write_png(icons / f"app-{name}.png", 256, 256, raster(256, rgb, True, True))
    assets = repo / "crates" / "wcr" / "assets"
    assets.mkdir(parents=True, exist_ok=True)
    write_png(assets / "icon.png", 256, 256, raster(256, accent, True, True))
    write_ico(assets / "icon.ico", [16, 24, 32, 48, 64, 256], accent)


if __name__ == "__main__":
    main()
