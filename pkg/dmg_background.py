#!/usr/bin/env python3
import math
import struct
import sys
import zlib

width, height = 1000, 640
pixels = bytearray()
for y in range(height):
    row = bytearray()
    for x in range(width):
        t = (x + height - y) / (width + height)
        r, g, b = int(224 - 12 * t), int(237 - 18 * t), int(248 - 22 * t)
        distance = math.hypot(x - width / 2, y - height / 2)
        if any(abs(distance - radius) < 1.2 for radius in range(80, 421, 85)):
            r, g, b = max(0, r - 22), max(0, g - 32), max(0, b - 45)
        row.extend((r, g, b, 255))
    pixels.extend(b"\x00" + row)

def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff)

png = b"\x89PNG\r\n\x1a\n"
png += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(bytes(pixels), 9))
png += chunk(b"IEND", b"")
with open(sys.argv[1], "wb") as output:
    output.write(png)
