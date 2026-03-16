# -*- coding: utf-8 -*-
"""Build-time icon generation helpers."""

# 标准库导入
from pathlib import Path

# 第三方库导入
from PIL import Image


ICON_SIZES = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (72, 72), (80, 80), (96, 96), (128, 128), (256, 256)]


def _needs_refresh(source_path: Path, target_path: Path) -> bool:
    if not target_path.exists():
        return True

    return source_path.stat().st_mtime > target_path.stat().st_mtime


def prepare_build_icon(project_root: Path, build_root: Path) -> str:
    """Generate the EXE icon from the same logo image used by the app UI."""

    source_png = project_root / "src" / "resource" / "icon" / "color_icon" / "logo.png"
    fallback_ico = project_root / "src" / "resource" / "icon" / "icon.ico"

    if not source_png.exists():
        return str(fallback_ico)

    target_dir = build_root / "generated_assets"
    target_dir.mkdir(parents=True, exist_ok=True)
    target_icon = target_dir / "app_icon.ico"

    if _needs_refresh(source_png, target_icon):
        with Image.open(source_png) as image:
            image.convert("RGBA").save(target_icon, format="ICO", sizes=ICON_SIZES)

    return str(target_icon)
