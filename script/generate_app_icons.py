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
# Windows 通知区常见逻辑像素：16（100%）、20/24（125%/150%）、32（200%）
TRAY_SIZES = (16, 20, 24, 32)

BRAND_500 = (255, 107, 61, 255)
BRAND_100 = (255, 233, 207, 255)
BRAND_50 = (255, 245, 236, 255)
UI_ASSETS = ROOT / "src-ui" / "assets"
# 侧栏 / 开屏用预缩放，避免 1024px 在 WebView 里缩到 24–72px 发糊
UI_LOGO_SIZES = (32, 48, 72)


def _oversample_resize(rgba: Image.Image, size: int) -> Image.Image:
    """高分辨率源 → 4x 超采样再落到目标，小图标边缘更利落。"""
    if size <= 0:
        raise ValueError(size)
    work = max(size * 4, 64)
    step = rgba.resize((work, work), Image.Resampling.LANCZOS)
    return step.resize((size, size), Image.Resampling.LANCZOS)


def _compose_tray_base(rgba: Image.Image, size: int) -> Image.Image:
    # 与 bundle 32x32 同思路：满幅 logo + 不透明底，避免小图再被系统二次缩小发糊
    logo = _oversample_resize(rgba, size)
    canvas = Image.new("RGBA", (size, size), BRAND_50)
    canvas.paste(logo, (0, 0), logo)
    flat = Image.new("RGBA", (size, size), BRAND_50)
    flat.paste(canvas, (0, 0), canvas.split()[3])
    rgb = flat.convert("RGB")
    return Image.merge("RGBA", (*rgb.split(), Image.new("L", (size, size), 255)))


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


def write_ui_logo_assets(rgba: Image.Image) -> None:
    UI_ASSETS.mkdir(parents=True, exist_ok=True)
    for size in UI_LOGO_SIZES:
        out = _oversample_resize(rgba, size)
        out.save(UI_ASSETS / f"logo-{size}.png", optimize=True)
    print(f"已写入 {UI_ASSETS}/logo-{{32,48,72}}.png")


def write_bundle_icons(rgba: Image.Image) -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    frames = [_oversample_resize(rgba, w) for w, h in SIZES]
    frames[0].save(ICO_PATH, format="ICO", sizes=[f.size for f in frames], append_images=frames[1:])
    for w, h in SIZES:
        if w >= 32:
            _oversample_resize(rgba, w).save(OUT_DIR / f"{w}x{h}.png", optimize=True)
    print(f"已写入 {ICO_PATH} 及 {OUT_DIR}/*.png")


def main() -> None:
    src = SOURCE if SOURCE.is_file() else FALLBACK
    if not src.is_file():
        raise SystemExit(f"找不到 logo：{SOURCE} 或 {FALLBACK}")

    with Image.open(src) as im:
        rgba = im.convert("RGBA")
        write_bundle_icons(rgba)
        write_tray_assets(rgba)
        write_ui_logo_assets(rgba)


if __name__ == "__main__":
    main()