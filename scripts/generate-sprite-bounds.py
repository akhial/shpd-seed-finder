#!/usr/bin/env python3
"""Measure the art bounding box of every 16x16 cell in the SHPD items sprite sheet.

The art is anchored to the top-left of each cell, so rendering a full cell
makes small items (rings, seeds) look off-center. This emits
web/src/generated/sprite-bounds.json mapping sprite index -> [x, y, w, h],
omitting empty cells. Pure-stdlib PNG decode; rerun when items.png changes.
"""
import json
import struct
import sys
import zlib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SHEET = REPO / 'web/public/third_party/shattered-pixel-dungeon/items.png'
OUTPUT = REPO / 'web/src/generated/sprite-bounds.json'
CELL = 16


def decode_png(path: Path):
    data = path.read_bytes()
    pos = 8
    idat = b''
    width = height = colortype = None
    trns = None
    while pos < len(data):
        length, ctype = struct.unpack('>I4s', data[pos:pos + 8])
        chunk = data[pos + 8:pos + 8 + length]
        if ctype == b'IHDR':
            width, height, bitdepth, colortype = struct.unpack('>IIBB', chunk[:10])
            if bitdepth != 8:
                sys.exit(f'unsupported bit depth {bitdepth}')
        elif ctype == b'IDAT':
            idat += chunk
        elif ctype == b'tRNS':
            trns = chunk
        pos += 12 + length
    raw = zlib.decompress(idat)
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[colortype]
    stride = width * channels
    rows = []
    prev = bytearray(stride)
    i = 0
    for _ in range(height):
        filter_type = raw[i]
        i += 1
        line = bytearray(raw[i:i + stride])
        i += stride
        if filter_type == 1:
            for x in range(channels, stride):
                line[x] = (line[x] + line[x - channels]) & 255
        elif filter_type == 2:
            for x in range(stride):
                line[x] = (line[x] + prev[x]) & 255
        elif filter_type == 3:
            for x in range(stride):
                left = line[x - channels] if x >= channels else 0
                line[x] = (line[x] + (left + prev[x]) // 2) & 255
        elif filter_type == 4:
            for x in range(stride):
                a = line[x - channels] if x >= channels else 0
                b = prev[x]
                c = prev[x - channels] if x >= channels else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(line)
        prev = line

    def alpha(x: int, y: int) -> int:
        if colortype == 6:
            return rows[y][x * 4 + 3]
        if colortype == 3:
            index = rows[y][x]
            return trns[index] if trns and index < len(trns) else 255
        return 255

    return width, height, alpha


def main() -> None:
    width, height, alpha = decode_png(SHEET)
    bounds = {}
    for index in range((width // CELL) * (height // CELL)):
        col, row = index % (width // CELL), index // (width // CELL)
        x0 = y0 = CELL
        x1 = y1 = -1
        for yy in range(CELL):
            for xx in range(CELL):
                if alpha(col * CELL + xx, row * CELL + yy) > 8:
                    x0, y0 = min(x0, xx), min(y0, yy)
                    x1, y1 = max(x1, xx), max(y1, yy)
        if x1 >= 0:
            bounds[str(index)] = [x0, y0, x1 - x0 + 1, y1 - y0 + 1]
    OUTPUT.write_text(json.dumps(bounds, separators=(',', ':')) + '\n')
    print(f'wrote {len(bounds)} sprite bounds to {OUTPUT}')


if __name__ == '__main__':
    main()
