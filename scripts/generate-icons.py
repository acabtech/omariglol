#!/usr/bin/env python3
"""Generate placeholder omariglol app icons.

These are stand-ins so `tauri build` has something to bundle. Replace them with
real artwork by running:

    npm run tauri icon path/to/logo.png

which produces the full platform icon set (including .ico and .icns).

Pure stdlib on purpose: no Pillow, no ImageMagick.
"""

from __future__ import annotations

import struct
import zlib
from pathlib import Path

# The fallback Omarchy accent, so the placeholder at least matches the default
# theme.
ACCENT = (0x7A, 0xA2, 0xF7)
BACKGROUND = (0x1A, 0x1B, 0x26)


def png_chunk(tag: bytes, data: bytes) -> bytes:
    return (
        struct.pack(">I", len(data))
        + tag
        + data
        + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    )


def write_png(path: Path, size: int) -> None:
    """Render a rounded square with a waveform glyph, RGBA."""
    radius = size * 0.22
    cx = cy = size / 2.0

    # A simple square wave across the middle, as a fraction of the icon size.
    wave_points = [
        (-0.30, 0.00),
        (-0.18, 0.00),
        (-0.18, -0.16),
        (-0.02, -0.16),
        (-0.02, 0.16),
        (0.14, 0.16),
        (0.14, -0.16),
        (0.30, -0.16),
        (0.30, 0.00),
    ]
    stroke = max(1.5, size * 0.055)

    rows = bytearray()
    for y in range(size):
        rows.append(0)  # filter type: none
        for x in range(size):
            px, py = x + 0.5, y + 0.5

            # Rounded-rectangle mask with a 1px antialiased edge.
            dx = max(radius - px, px - (size - radius), 0.0)
            dy = max(radius - py, py - (size - radius), 0.0)
            dist = (dx * dx + dy * dy) ** 0.5
            inside = 1.0 if dist <= radius - 1 else max(0.0, radius - dist)

            if inside <= 0.0:
                rows.extend((0, 0, 0, 0))
                continue

            # Distance from the waveform polyline, in icon coordinates.
            best = 1e9
            for (x0f, y0f), (x1f, y1f) in zip(wave_points, wave_points[1:]):
                ax, ay = cx + x0f * size, cy + y0f * size
                bx, by = cx + x1f * size, cy + y1f * size
                vx, vy = bx - ax, by - ay
                wx, wy = px - ax, py - ay
                seg = vx * vx + vy * vy
                t = 0.0 if seg == 0 else max(0.0, min(1.0, (wx * vx + wy * vy) / seg))
                ddx, ddy = px - (ax + t * vx), py - (ay + t * vy)
                best = min(best, (ddx * ddx + ddy * ddy) ** 0.5)

            on_wave = 1.0 if best <= stroke else max(0.0, 1.0 - (best - stroke))
            r = BACKGROUND[0] * on_wave + ACCENT[0] * (1 - on_wave)
            g = BACKGROUND[1] * on_wave + ACCENT[1] * (1 - on_wave)
            b = BACKGROUND[2] * on_wave + ACCENT[2] * (1 - on_wave)

            rows.extend((int(r), int(g), int(b), int(255 * inside)))

    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    png = (
        b"\x89PNG\r\n\x1a\n"
        + png_chunk(b"IHDR", ihdr)
        + png_chunk(b"IDAT", zlib.compress(bytes(rows), 9))
        + png_chunk(b"IEND", b"")
    )
    path.write_bytes(png)
    print(f"  {path} ({size}x{size}, {len(png)} bytes)")


def main() -> None:
    out = Path(__file__).resolve().parent.parent / "src-tauri" / "icons"
    out.mkdir(parents=True, exist_ok=True)
    print("generating placeholder icons:")
    for name, size in [
        ("32x32.png", 32),
        ("128x128.png", 128),
        ("128x128@2x.png", 256),
        ("icon.png", 512),
    ]:
        write_png(out / name, size)
    print("done — replace these with `npm run tauri icon <logo.png>`")


if __name__ == "__main__":
    main()
