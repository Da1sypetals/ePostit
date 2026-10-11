import math
import subprocess
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets"
SRC = ASSETS / "icon-src.png"

# Apple 图标规范:内容区约占画布 80%,外形为连续曲率圆角矩形(超椭圆近似)
CONTENT_RATIO = 0.80
SUPERELLIPSE_N = 5.0
SUPERSAMPLE = 4

# 菜单栏模板图标:与背景差异超过阈值的部分视为图形
MASK_THRESHOLD = 12
MASK_GAIN = 6.0
TRAY_SIZE = 44


def squircle_mask(size: int) -> Image.Image:
    s = size * SUPERSAMPLE
    mask = Image.new("L", (s, s), 0)
    draw = ImageDraw.Draw(mask)
    a = s / 2
    points = []
    steps = s * 4
    for i in range(steps + 1):
        t = 2 * math.pi * i / steps
        ct, st = math.cos(t), math.sin(t)
        r = a / (abs(ct) ** SUPERELLIPSE_N + abs(st) ** SUPERELLIPSE_N) ** (
            1 / SUPERELLIPSE_N
        )
        points.append((a + r * ct, a + r * st))
    draw.polygon(points, fill=255)
    return mask.resize((size, size), Image.LANCZOS)


def make_app_icon() -> Image.Image:
    src = Image.open(SRC).convert("RGBA")
    bg = src.getpixel((4, 4))[:3]
    size = 1024
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    plate = Image.new("RGBA", (size, size), (*bg, 255))
    canvas.paste(plate, (0, 0), squircle_mask(size))
    content = int(size * CONTENT_RATIO)
    scaled = src.resize((content, content), Image.LANCZOS)
    off = (size - content) // 2
    canvas.paste(scaled, (off, off))
    return canvas


def make_iconset(master: Image.Image) -> None:
    iconset = ASSETS / "epostit.iconset"
    iconset.mkdir(exist_ok=True)
    for base, scale in [(16, 1), (16, 2), (32, 1), (32, 2), (128, 1), (128, 2), (256, 1), (256, 2), (512, 1), (512, 2)]:
        px = base * scale
        name = f"icon_{base}x{base}.png" if scale == 1 else f"icon_{base}x{base}@2x.png"
        master.resize((px, px), Image.LANCZOS).save(iconset / name)
    icns = ASSETS / "AppIcon.icns"
    if icns.exists():
        icns.unlink()
    subprocess.run(
        ["iconutil", "-c", "icns", str(iconset), "-o", str(icns)], check=True
    )


def make_tray_mask() -> Image.Image:
    src = Image.open(SRC).convert("RGB")
    bg = src.getpixel((4, 4))
    w, h = src.size
    mask = Image.new("L", (w, h), 0)
    px_src, px_dst = src.load(), mask.load()
    for y in range(h):
        for x in range(w):
            c = px_src[x, y]
            diff = max(abs(c[i] - bg[i]) for i in range(3))
            px_dst[x, y] = min(255, max(0, int((diff - MASK_THRESHOLD) * MASK_GAIN)))
    bbox = mask.getbbox()
    mask = mask.crop(bbox)
    side = max(mask.size)
    square = Image.new("L", (side, side), 0)
    square.paste(mask, ((side - mask.width) // 2, (side - mask.height) // 2))
    square = square.resize((TRAY_SIZE, TRAY_SIZE), Image.LANCZOS)
    rgba = Image.new("RGBA", (TRAY_SIZE, TRAY_SIZE), (0, 0, 0, 0))
    black = Image.new("RGBA", (TRAY_SIZE, TRAY_SIZE), (0, 0, 0, 255))
    rgba.paste(black, (0, 0), square)
    return rgba


def main() -> None:
    master = make_app_icon()
    master.save(ASSETS / "app-icon-1024.png")
    make_iconset(master)
    tray = make_tray_mask()
    tray.save(ASSETS / "tray-icon.png")
    (ASSETS / "tray-icon.rgba").write_bytes(tray.tobytes())
    print("done:", sorted(p.name for p in ASSETS.iterdir()))


main()
