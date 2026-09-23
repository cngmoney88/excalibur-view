#!/usr/bin/env python3
"""Makes every icon the app needs from one square PNG.

    python3 tools/make_icons.py assets/icon-1024.png

Writes deploy/macos/AppIcon.icns. Add --windows to write assets/hyperview.ico
as well, which changes the icon on an app people already have installed, so it
is deliberate rather than automatic.

It needs nothing but Pillow, so it runs the same on Windows, Linux and a Mac
-- you do not need iconutil, and you do not need to be on a Mac to make a Mac
icon.
"""
import struct
import sys
from io import BytesIO
from pathlib import Path

from PIL import Image

# The chunk types macOS reads, and the pixel size each one holds. The pairs
# that share a size are not a mistake: 128@2x and 256 are both 256 pixels,
# and macOS wants them under both names.
ICNS_TYPES = [
    (b"icp4", 16),
    (b"icp5", 32),
    (b"ic11", 32),    # 16@2x
    (b"ic12", 64),    # 32@2x
    (b"ic07", 128),
    (b"ic13", 256),   # 128@2x
    (b"ic08", 256),
    (b"ic14", 512),   # 256@2x
    (b"ic09", 512),
    (b"ic10", 1024),  # 512@2x
]

# Windows asks for 20 and 40 as well, for its own display scalings.
ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]


def scaled(src: Image.Image, size: int) -> Image.Image:
    return src.resize((size, size), Image.LANCZOS)


def as_png(im: Image.Image) -> bytes:
    buf = BytesIO()
    im.save(buf, format="PNG")
    return buf.getvalue()


def write_icns(src: Image.Image, out: Path) -> None:
    chunks = b""
    for kind, size in ICNS_TYPES:
        data = as_png(scaled(src, size))
        chunks += kind + struct.pack(">I", len(data) + 8) + data
    out.write_bytes(b"icns" + struct.pack(">I", len(chunks) + 8) + chunks)


def write_ico(src: Image.Image, out: Path) -> None:
    src.save(out, format="ICO", sizes=[(s, s) for s in ICO_SIZES])


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    args = [a for a in sys.argv[1:] if a != "--windows"]
    also_windows = "--windows" in sys.argv[1:]
    source = Path(args[0]) if args else root / "assets/icon-1024.png"
    src = Image.open(source).convert("RGBA")
    if src.width != src.height:
        print(f"{source} is {src.width}x{src.height}; it has to be square")
        return 1
    if src.width < 1024:
        print(f"{source} is only {src.width} across; 1024 or more makes a sharp icon")

    icns = root / "deploy/macos/AppIcon.icns"
    write_icns(src, icns)
    print(f"{icns.relative_to(root)}  {icns.stat().st_size:,} bytes")
    if also_windows:
        ico = root / "assets/hyperview.ico"
        write_ico(src, ico)
        print(f"{ico.relative_to(root)}  {ico.stat().st_size:,} bytes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
