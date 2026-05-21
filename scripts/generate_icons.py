"""
Generate Void app icons with:
- iOS/macOS-style squircle (superellipse) background, dark #16171a
- White [/] logo centered at ~60% scale
- Correct dimensions from nir_logo.svg (stroke-width 10/100 = 10% of logo size)

Outputs:
  crates/zed/resources/app-icon.png       (512x512)
  crates/zed/resources/app-icon@2x.png   (1024x1024)
  + -dev, -nightly, -preview variants
  crates/zed/resources/windows/app-icon.ico  (multi-size .ico)
  + -dev, -nightly, -preview variants
"""

import math
import struct
import zlib
import io
import os
import sys

# ---------------------------------------------------------------------------
# Pure-Python PNG writer (no dependencies)
# ---------------------------------------------------------------------------

def _write_chunk(buf, chunk_type, data):
    length = len(data)
    buf.write(struct.pack('>I', length))
    crc_data = chunk_type + data
    buf.write(crc_data)
    buf.write(struct.pack('>I', zlib.crc32(crc_data) & 0xFFFFFFFF))

def save_png(pixels, width, height, path):
    """Save RGBA pixel list (flat, row-major) as PNG."""
    buf = io.BytesIO()
    buf.write(b'\x89PNG\r\n\x1a\n')

    # IHDR
    _write_chunk(buf, b'IHDR',
                 struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0))

    # IDAT
    raw_rows = []
    for y in range(height):
        row = bytearray([0])  # filter type None
        for x in range(width):
            idx = (y * width + x) * 4
            r, g, b, a = pixels[idx], pixels[idx+1], pixels[idx+2], pixels[idx+3]
            row += bytearray([r, g, b])
        raw_rows.append(bytes(row))
    compressed = zlib.compress(b''.join(raw_rows), 9)
    _write_chunk(buf, b'IDAT', compressed)

    _write_chunk(buf, b'IEND', b'')

    with open(path, 'wb') as f:
        f.write(buf.getvalue())
    print(f'  Saved: {path}')


def save_png_rgba(pixels, width, height, path):
    """Save RGBA pixel list (flat, row-major) as RGBA PNG."""
    buf = io.BytesIO()
    buf.write(b'\x89PNG\r\n\x1a\n')

    # IHDR — colour type 6 = RGBA
    _write_chunk(buf, b'IHDR',
                 struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0))

    raw_rows = []
    for y in range(height):
        row = bytearray([0])
        for x in range(width):
            idx = (y * width + x) * 4
            row += bytearray(pixels[idx:idx+4])
        raw_rows.append(bytes(row))
    compressed = zlib.compress(b''.join(raw_rows), 9)
    _write_chunk(buf, b'IDAT', compressed)
    _write_chunk(buf, b'IEND', b'')

    with open(path, 'wb') as f:
        f.write(buf.getvalue())
    print(f'  Saved: {path}')


# ---------------------------------------------------------------------------
# Shape helpers
# ---------------------------------------------------------------------------

def inside_rounded_rect(px, py, x0, y0, x1, y1, rx, ry):
    """Returns True if point is strictly inside a rounded rectangle."""
    if px < x0 or px > x1 or py < y0 or py > y1:
        return False
    # Corner circle centres
    cx_left  = x0 + rx
    cx_right = x1 - rx
    cy_top   = y0 + ry
    cy_bot   = y1 - ry
    if px < cx_left and py < cy_top:
        return math.hypot(px - cx_left,  py - cy_top) <= rx
    if px > cx_right and py < cy_top:
        return math.hypot(px - cx_right, py - cy_top) <= rx
    if px < cx_left and py > cy_bot:
        return math.hypot(px - cx_left,  py - cy_bot) <= rx
    if px > cx_right and py > cy_bot:
        return math.hypot(px - cx_right, py - cy_bot) <= rx
    return True


def rounded_rect_coverage(px, py, x0, y0, x1, y1, rx, ry, samples=4):
    """Anti-aliased coverage for a rounded rectangle."""
    step  = 1.0 / samples
    start = step * 0.5
    count = 0
    for sy in range(samples):
        for sx in range(samples):
            if inside_rounded_rect(px + start + sx*step,
                                   py + start + sy*step,
                                   x0, y0, x1, y1, rx, ry):
                count += 1
    return count / (samples * samples)


# ---------------------------------------------------------------------------
# Logo drawing (the [/] mark)
# ---------------------------------------------------------------------------

def _dist_point_to_segment(px, py, ax, ay, bx, by):
    """Minimum distance from point (px,py) to segment (ax,ay)-(bx,by)."""
    dx, dy = bx - ax, by - ay
    if dx == 0 and dy == 0:
        return math.hypot(px - ax, py - ay)
    t = max(0.0, min(1.0, ((px - ax)*dx + (py - ay)*dy) / (dx*dx + dy*dy)))
    nx, ny = ax + t*dx, ay + t*dy
    return math.hypot(px - nx, py - ny)


def _segment_coverage_square_cap(px, py, ax, ay, bx, by, half_w, samples=4):
    """Anti-aliased coverage for a thick segment with SQUARE caps.
    Square caps mean the stroke extends half_w beyond each endpoint,
    perpendicular to the segment direction. We compute this via a rotated
    rectangle (OBB) test."""
    dx = bx - ax
    dy = by - ay
    length = math.hypot(dx, dy)
    if length == 0:
        return 0.0
    # Unit along-segment and perpendicular vectors
    ux, uy = dx / length, dy / length   # along
    vx, vy = -uy, ux                    # perpendicular (left normal)

    step = 1.0 / samples
    start = step * 0.5
    count = 0
    for sy in range(samples):
        for sx in range(samples):
            spx = px + start + sx * step
            spy = py + start + sy * step
            # Transform to segment-local coords
            relx = spx - ax
            rely = spy - ay
            along = relx * ux + rely * uy
            perp  = relx * vx + rely * vy
            # Square cap: along in [-half_w, length+half_w], perp in [-half_w, half_w]
            if -half_w <= along <= length + half_w and abs(perp) <= half_w:
                count += 1
    return count / (samples * samples)


def _rect_stroke_coverage(px, py, x0, y0, x1, y1, half_w, samples=4):
    """Coverage for a 4-sided rectangle stroke (axis-aligned)."""
    # Four segments: top, bottom, left, right
    step = 1.0 / samples
    start = step * 0.5
    count = 0
    segs = [
        (x0, y0, x1, y0),  # top
        (x0, y1, x1, y1),  # bottom
        (x0, y0, x0, y1),  # left
        (x1, y0, x1, y1),  # right
    ]
    for sy in range(samples):
        for sx in range(samples):
            spx = px + start + sx * step
            spy = py + start + sy * step
            hit = False
            for (ax, ay, bx, by) in segs:
                if _dist_point_to_segment(spx, spy, ax, ay, bx, by) <= half_w:
                    hit = True
                    break
            if hit:
                count += 1
    return count / (samples * samples)


# ---------------------------------------------------------------------------
# Render one icon — full-bleed background with larger [/] mark:
#
#   <rect x="0" y="0" width="100" height="100" rx="18" fill="#0a0a0b"/>
#   <rect x="17" y="17" width="66" height="66" stroke="#fff" stroke-width="10"/>
#   <path d="M39 61 L61 39" stroke="#fff" stroke-width="10" stroke-linecap="square"/>
#
# All coordinates are in the 100x100 SVG unit space, scaled to pixel space.
# ---------------------------------------------------------------------------

def render_icon(size, bg_color=(0x0a, 0x0a, 0x0b), logo_color=(0xFF, 0xFF, 0xFF)):
    """
    Render a size x size RGBA icon.

    SVG coordinate space is 0..100 units. Each unit = size/100 pixels.

    Background: full-bleed rounded rect (0,0) 100x100 rx=18, fill #0a0a0b
    Logo [/]:   no scale transform:
      Square rect (17,17)-(83,83) sw=10
      Slash M39 61 L61 39 sw=10 (stroke-linecap=square)
    """
    S = size
    s = S / 100.0   # pixels per SVG unit

    # ---- Background rounded rect (full-bleed) ----
    bg_x0 = 0 * s;   bg_y0 = 0 * s
    bg_x1 = 100 * s; bg_y1 = 100 * s
    bg_rx = 18 * s;  bg_ry = 18 * s

    # ---- Logo geometry (centre of stroke lines) ----
    # Square: 66x66 centred at (50,50) -> (17,17)-(83,83)
    sq_x0 = 17 * s;  sq_y0 = 17 * s
    sq_x1 = 83 * s;  sq_y1 = 83 * s
    logo_hw = 5.0 * s   # half of stroke-width 10

    # Slash: diagonal through centre (no scale applied)
    sax = 39 * s;  say = 61 * s
    sbx = 61 * s;  sby = 39 * s

    pixels = bytearray(S * S * 4)

    for y in range(S):
        for x in range(S):
            # ---- Background coverage (rounded rect) ----
            bg_cov = rounded_rect_coverage(x, y, bg_x0, bg_y0, bg_x1, bg_y1,
                                           bg_rx, bg_ry, samples=4)
            if bg_cov <= 0.0:
                idx = (y * S + x) * 4
                pixels[idx:idx+4] = bytes([0, 0, 0, 0])
                continue

            # ---- Logo coverage ----
            rect_cov  = _rect_stroke_coverage(x, y, sq_x0, sq_y0, sq_x1, sq_y1,
                                              logo_hw, samples=4)
            slash_cov = _segment_coverage_square_cap(x, y, sax, say, sbx, sby,
                                                     logo_hw, samples=4)
            logo_cov  = min(1.0, rect_cov + slash_cov)

            # Composite logo over background; alpha = bg_cov for AA edges
            r = int(bg_color[0] * (1 - logo_cov) + logo_color[0] * logo_cov)
            g = int(bg_color[1] * (1 - logo_cov) + logo_color[1] * logo_cov)
            b = int(bg_color[2] * (1 - logo_cov) + logo_color[2] * logo_cov)
            a = int(255 * bg_cov)

            idx = (y * S + x) * 4
            pixels[idx]   = r
            pixels[idx+1] = g
            pixels[idx+2] = b
            pixels[idx+3] = a

    return pixels


# ---------------------------------------------------------------------------
# ICO writer
# ---------------------------------------------------------------------------

def make_ico(png_bytes_list):
    """Combine multiple PNG byte-strings into a .ico file."""
    n = len(png_bytes_list)
    header = struct.pack('<HHH', 0, 1, n)   # reserved, type=1(ICO), count
    dir_entries = b''
    offset = 6 + n * 16

    infos = []
    for png_bytes in png_bytes_list:
        # Parse PNG dimensions
        w = struct.unpack('>I', png_bytes[16:20])[0]
        h = struct.unpack('>I', png_bytes[20:24])[0]
        size = len(png_bytes)
        infos.append((w, h, size, offset))
        # ICO dir entry: width(1), height(1), colorCount(1), reserved(1),
        #                planes(2), bitCount(2), bytesInRes(4), imageOffset(4)
        w_byte = w if w < 256 else 0
        h_byte = h if h < 256 else 0
        dir_entries += struct.pack('<BBBBHHII', w_byte, h_byte, 0, 0, 1, 32, size, offset)
        offset += size

    return header + dir_entries + b''.join(png_bytes_list)


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def png_bytes(pixels, w, h):
    buf = io.BytesIO()
    # IHDR colour type 6 = RGBA
    def wchunk(t, d):
        buf.write(struct.pack('>I', len(d)))
        crc_d = t + d
        buf.write(crc_d)
        buf.write(struct.pack('>I', zlib.crc32(crc_d) & 0xFFFFFFFF))

    buf.write(b'\x89PNG\r\n\x1a\n')
    wchunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 6, 0, 0, 0))
    rows = []
    for y in range(h):
        rows.append(b'\x00' + bytes(pixels[y*w*4:(y+1)*w*4]))
    wchunk(b'IDAT', zlib.compress(b''.join(rows), 9))
    wchunk(b'IEND', b'')
    return buf.getvalue()


def generate_all():
    base = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    res_dir  = os.path.join(base, 'crates', 'zed', 'resources')
    win_dir  = os.path.join(res_dir, 'windows')

    # Each channel: (suffix, badge_color or None)
    # We keep all variants visually identical for now (dark bg, white logo)
    channels = ['', '-dev', '-nightly', '-preview']

    print('=== Generating Void app icons ===')
    print(f'Output base: {res_dir}')

    # Render at 512 and 1024
    print('\nRendering 512×512...')
    px512  = render_icon(512)
    print('Rendering 1024×1024...')
    px1024 = render_icon(1024)

    # Also render smaller sizes for ICO
    print('Rendering 256×256...')
    px256  = render_icon(256)
    print('Rendering 128×128...')
    px128  = render_icon(128)
    print('Rendering 64×64...')
    px64   = render_icon(64)
    print('Rendering 32×32...')
    px32   = render_icon(32)
    print('Rendering 16×16...')
    px16   = render_icon(16)

    png512  = png_bytes(px512,  512,  512)
    png1024 = png_bytes(px1024, 1024, 1024)
    png256  = png_bytes(px256,  256,  256)
    png128  = png_bytes(px128,  128,  128)
    png64   = png_bytes(px64,   64,   64)
    png32   = png_bytes(px32,   32,   32)
    png16   = png_bytes(px16,   16,   16)

    print('\nSaving PNG files...')
    for suffix in channels:
        p1 = os.path.join(res_dir, f'app-icon{suffix}.png')
        p2 = os.path.join(res_dir, f'app-icon{suffix}@2x.png')
        with open(p1, 'wb') as f: f.write(png512)
        print(f'  Saved: {p1}')
        with open(p2, 'wb') as f: f.write(png1024)
        print(f'  Saved: {p2}')

    print('\nSaving ICO files...')
    ico_data = make_ico([png16, png32, png64, png128, png256])
    for suffix in channels:
        ico_path = os.path.join(win_dir, f'app-icon{suffix}.ico')
        with open(ico_path, 'wb') as f: f.write(ico_data)
        print(f'  Saved: {ico_path}')

    print('\nAll icons generated successfully!')
    print('\nSizes produced:')
    print(f'  PNG 512x512  : {len(png512):,} bytes')
    print(f'  PNG 1024x1024: {len(png1024):,} bytes')
    print(f'  ICO (multi)  : {len(ico_data):,} bytes')


if __name__ == '__main__':
    generate_all()
