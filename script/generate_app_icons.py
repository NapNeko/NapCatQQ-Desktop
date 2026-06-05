#!/usr/bin/env python3
"""从 UI logo 生成 src-tauri/icons 下的 Windows ICO（与旧版 build_icon.py 同思路）。"""

from __future__ import annotations

from pathlib import Path

try:
    from PIL import Image
except ImportError as e:
    raise SystemExit(
        "需要 Pillow：pip install pillow\n"
        "或在本机执行：pnpm tauri icon src-ui/assets/logo.png"
    ) from e

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "src-ui" / "assets" / "logo.png"
FALLBACK = ROOT / ".references" / "legacy-python" / "src" / "resource" / "icon" / "color_icon" / "logo.png"
OUT_DIR = ROOT / "src-tauri" / "icons"
ICO_PATH = OUT_DIR / "icon.ico"

SIZES = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]


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

    print(f"已写入 {ICO_PATH} 及 {OUT_DIR}/*.png")


if __name__ == "__main__":
    main()