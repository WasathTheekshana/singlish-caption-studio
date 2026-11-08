"""Draw the app icon (icons/icon.png and icons/icon.ico) using only the standard library.

Run from the repository root:  python tools/make_icon.py
The icon is a dark rounded square with two light bars, like lines of a caption.
"""
import struct
import zlib
from pathlib import Path

SIZE = 256
RADIUS = 56
INK = (28, 27, 25)
PAPER = (246, 243, 238)
# (left, right, top, bottom) of the two caption bars
BARS = ((52, 204, 120, 148), (84, 172, 164, 192))
BAR_RADIUS = 14


def corner_distance(x, y, r):
    """Distance from the corner circle's centre when (x, y) is inside a corner, else None."""
    cx, cy = min(x, SIZE - 1 - x), min(y, SIZE - 1 - y)
    if cx < r and cy < r:
        return ((r - cx) ** 2 + (r - cy) ** 2) ** 0.5
    return None


def pixel(x, y):
    d = corner_distance(x, y, RADIUS)
    if d is None:
        alpha = 255
    elif d > RADIUS + 0.5:
        return (0, 0, 0, 0)
    else:
        alpha = 255 if d <= RADIUS - 0.5 else int(255 * (RADIUS + 0.5 - d))
    for left, right, top, bottom in BARS:
        if left <= x < right and top <= y < bottom:
            ex, ey = min(x - left, right - 1 - x), min(y - top, bottom - 1 - y)
            if ex < BAR_RADIUS and ey < BAR_RADIUS and ((BAR_RADIUS - ex) ** 2 + (BAR_RADIUS - ey) ** 2) ** 0.5 > BAR_RADIUS:
                continue
            return (*PAPER, alpha)
    return (*INK, alpha)


def chunk(tag, data):
    body = tag + data
    return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)


def main():
    rows = b"".join(b"\x00" + b"".join(bytes(pixel(x, y)) for x in range(SIZE)) for y in range(SIZE))
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(rows, 9))
        + chunk(b"IEND", b"")
    )
    out = Path(__file__).resolve().parent.parent / "icons"
    out.mkdir(exist_ok=True)
    (out / "icon.png").write_bytes(png)
    # A one-image .ico that holds the PNG as is (Windows Vista and newer read this fine).
    header = struct.pack("<HHH", 0, 1, 1) + struct.pack("<BBBBHHII", 0, 0, 0, 0, 1, 32, len(png), 22)
    (out / "icon.ico").write_bytes(header + png)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
