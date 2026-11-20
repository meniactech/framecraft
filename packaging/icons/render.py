#!/usr/bin/env python3
# Renders packaging/linux/framecraft.svg (the master icon) to the PNG sizes
# and the Windows .ico in this folder. Run again after changing the SVG.
# Needs: python3-gi with GdkPixbuf and its SVG loader (librsvg).
import os
import struct

import gi

gi.require_version("GdkPixbuf", "2.0")
from gi.repository import GdkPixbuf  # noqa: E402

here = os.path.dirname(os.path.abspath(__file__))
svg = os.path.join(here, "..", "linux", "framecraft.svg")
sizes = [16, 24, 32, 48, 64, 128, 256, 512, 1024]
pngs = {}
for size in sizes:
    pb = GdkPixbuf.Pixbuf.new_from_file_at_size(svg, size, size)
    path = os.path.join(here, f"framecraft-{size}.png")
    pb.savev(path, "png", [], [])
    with open(path, "rb") as f:
        pngs[size] = f.read()

# Windows .ico: PNG-compressed entries (Vista and newer).
ico_sizes = [16, 24, 32, 48, 64, 128, 256]
header = struct.pack("<HHH", 0, 1, len(ico_sizes))
offset = 6 + 16 * len(ico_sizes)
entries, blobs = b"", b""
for size in ico_sizes:
    data = pngs[size]
    dim = 0 if size >= 256 else size
    entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
    blobs += data
    offset += len(data)
with open(os.path.join(here, "framecraft.ico"), "wb") as f:
    f.write(header + entries + blobs)
print("Rendered", ", ".join(str(s) for s in sizes), "and framecraft.ico")
