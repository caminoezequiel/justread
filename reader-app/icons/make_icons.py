"""Draw the justread app icon (an open book on a blue rounded square) and
write it as PNGs plus a Windows .ico, using only the Python standard library.

Run from anywhere:  python3 reader-app/icons/make_icons.py
"""
import struct
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parent
SIZE = 512
SAMPLES = 3  # supersampling per axis, for smooth edges

BLUE = (59, 110, 168)
PAGE = (250, 250, 247)
LINE = (178, 196, 219)
SPINE = (36, 74, 120)


def rounded_square(x, y, r=104, m=24):
    lo, hi = m, SIZE - m
    if not (lo <= x <= hi and lo <= y <= hi):
        return False
    cx = min(max(x, lo + r), hi - r)
    cy = min(max(y, lo + r), hi - r)
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r


def page(x, y, left):
    """One page: a quad whose top and bottom edges dip towards the spine."""
    mid = SIZE / 2
    if left:
        if not (104 <= x <= mid - 6):
            return False
        t = (x - 104) / (mid - 6 - 104)  # 0 at the outer edge, 1 at the spine
    else:
        if not (mid + 6 <= x <= SIZE - 104):
            return False
        t = (SIZE - 104 - x) / (SIZE - 104 - (mid + 6))
    top = 150 + 34 * t * t
    bottom = 382 + 24 * t * t
    return top <= y <= bottom


def text_line(x, y, left):
    """Faint lines of 'text' on each page."""
    mid = SIZE / 2
    inner = (140, mid - 36) if left else (mid + 36, SIZE - 140)
    if not (inner[0] <= x <= inner[1]):
        return False
    t = abs(x - mid) / (mid - 104)
    t = 1 - t
    for row in range(6):
        base = 196 + row * 30 + 34 * t * t * 0.8
        if base <= y <= base + 9:
            return True
    return False


def color_at(x, y):
    if not rounded_square(x, y):
        return None
    mid = SIZE / 2
    if abs(x - mid) <= 6 and 170 <= y <= 404:
        return SPINE
    for left in (True, False):
        if page(x, y, left):
            return LINE if text_line(x, y, left) else PAGE
    return BLUE


def render():
    """RGBA rows at SIZE x SIZE, averaging SAMPLES^2 points per pixel."""
    rows = []
    step = 1 / SAMPLES
    for py in range(SIZE):
        row = []
        for px in range(SIZE):
            r = g = b = a = 0
            for sy in range(SAMPLES):
                for sx in range(SAMPLES):
                    c = color_at(px + (sx + 0.5) * step, py + (sy + 0.5) * step)
                    if c is not None:
                        r += c[0]; g += c[1]; b += c[2]; a += 255
            n = SAMPLES * SAMPLES
            if a:
                covered = a // 255
                row.append((r // covered, g // covered, b // covered, a // n))
            else:
                row.append((0, 0, 0, 0))
        rows.append(row)
    return rows


def downscale(rows, size):
    """Box-filter resize (premultiplied, so edges don't darken)."""
    src = len(rows)
    out = []
    for y in range(size):
        y0, y1 = y * src // size, (y + 1) * src // size
        row = []
        for x in range(size):
            x0, x1 = x * src // size, (x + 1) * src // size
            r = g = b = a = 0
            for yy in range(y0, y1):
                for xx in range(x0, x1):
                    pr, pg, pb, pa = rows[yy][xx]
                    r += pr * pa; g += pg * pa; b += pb * pa; a += pa
            n = (y1 - y0) * (x1 - x0)
            row.append((r // a, g // a, b // a, a // n) if a else (0, 0, 0, 0))
        out.append(row)
    return out


def png_bytes(rows):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    size = len(rows)
    raw = b"".join(b"\x00" + bytes(v for px in row for v in px) for row in rows)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def ico_bytes(pngs):
    """An .ico whose entries are PNG images (supported since Windows Vista)."""
    header = struct.pack("<HHH", 0, 1, len(pngs))
    entries, offset = b"", 6 + 16 * len(pngs)
    for size, data in pngs:
        dim = 0 if size >= 256 else size  # 0 means 256 in the ICO format
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    return header + entries + b"".join(data for _, data in pngs)


def main():
    full = render()
    pngs = {}
    for size in (16, 32, 48, 128, 256, 512):
        rows = full if size == SIZE else downscale(full, size)
        pngs[size] = png_bytes(rows)
    for size in (32, 128, 256, 512):
        (OUT / f"{size}x{size}.png").write_bytes(pngs[size])
    (OUT / "icon.ico").write_bytes(ico_bytes([(s, pngs[s]) for s in (16, 32, 48, 256)]))
    print("wrote icons to", OUT)


main()
