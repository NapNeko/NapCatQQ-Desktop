#!/usr/bin/env python3
"""从 UI logo 生成 src-tauri/icons 下的 Windows ICO（与旧版 build_icon.py 同思路）。"""

from __future__ import annotations

from pathlib import Path

try:
    from PIL import Image, ImageDraw, ImageOps
except ImportError as e:
    raise SystemExit(
        "需要 Pillow：pip install pillow\n"
        "或在本机执行：pnpm tauri icon src-ui/assets/logo.png"
    ) from e

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "src-ui" / "assets" / "logo.png"
FALLBACK = ROOT / ".references" / "legacy-python" / "src" / "resource" / "icon" / "color_icon" / "logo.png"
OUT_DIR = ROOT / "src-tauri" / "icons"
TRAY_DIR = OUT_DIR / "tray"
ICO_PATH = OUT_DIR / "icon.ico"

SIZES = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
TRAY_SIZES = (16, 24, 32)

BRAND_500 = (255, 107, 61, 255)
BRAND_100 = (255, 233, 207, 255)
BRAND_50 = (255, 245, 236, 255)


def _round_mask(size: int, radius_ratio: float = 0.22) -> Image.Image:
    mask = Image.new("L", (size, size), 0)
    draw = ImageDraw.Draw(mask)
    r = max(2, int(size * radius_ratio))
    draw.rounded_rectangle((0, 0, size - 1, size - 1), radius=r, fill=255)
    return mask


def _compose_tray_base(rgba: Image.Image, size: int) -> Image.Image:
    src = rgba.resize((size, size), Image.Resampling.LANCZOS)
    # 任务栏小图：轻暖底 + logo，圆角略大，偏「产品图标」而非贴纸
    plate = Image.new("RGBA", (size, size), BRAND_50)
    plate.putalpha(220)
    glow = Image.new("RGBA", (size, size), BRAND_100)
    glow.putalpha(int(255 * 0.28))
    out = Image.alpha_composite(plate, glow)
    out = Image.alpha_composite(out, src)
    mask = _round_mask(size, radius_ratio=0.26)
    rounded = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    rounded.paste(out, (0, 0), mask)
    return rounded


def _with_status_dot(base: Image.Image, color: tuple[int, int, int, int]) -> Image.Image:
    size = base.size[0]
    out = base.copy()
    draw = ImageDraw.Draw(out)
    d = max(4, size // 5)
    margin = max(1, size // 16)
    x0, y0 = size - d - margin, size - d - margin
    draw.ellipse((x0, y0, x0 + d, y0 + d), fill=color)
    return out


def _desaturate_light(img: Image.Image) -> Image.Image:
    gray = ImageOps.grayscale(img.convert("RGB")).convert("RGBA")
    gray.putalpha(img.split()[3])
    return Image.blend(img, gray, 0.35)


def write_tray_assets(rgba: Image.Image) -> None:
    TRAY_DIR.mkdir(parents=True, exist_ok=True)
    for size in TRAY_SIZES:
        base = _compose_tray_base(rgba, size)
        base.save(TRAY_DIR / f"tray-{size}.png")
        _with_status_dot(base, BRAND_500).save(TRAY_DIR / f"tray-active-{size}.png")
        _desaturate_light(base).save(TRAY_DIR / f"tray-light-{size}.png")
    print(f"已写入 {TRAY_DIR}/tray-*.png")


def main() -> None:
    src = SOURCE if SOURCE.is_file() else FALLBACK
    if not src.is_file():
        raise SystemExit(f"找不到 logo：{SOURCE} 或 {FALLBACK}")

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    with Image.open(src) as im:
        rgba = im.convert("RGBA")
        rgba.save(ICO_PATH, format="ICO", sizes=SIZES)
        for w, h in SIZES:
            if w >= 32:
                rgba.resize((w, h), Image.Resampling.LANCZOS).save(OUT_DIR / f"{w}x{h}.png")

    write_tray_assets(rgba)

    print(f"已写入 {ICO_PATH} 及 {OUT_DIR}/*.png")


if __name__ == "__main__":
    main()