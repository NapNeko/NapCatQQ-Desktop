#!/usr/bin/env python3
"""从定稿主图与托盘矢量稿生成应用品牌资源。"""

from __future__ import annotations

import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

try:
    from PIL import Image, ImageDraw, ImageOps
except ImportError as error:
    raise SystemExit("需要 Pillow：python -m pip install Pillow") from error

ROOT = Path(__file__).resolve().parents[1]
UI_ASSETS = ROOT / "src-ui" / "assets"
SOURCE = UI_ASSETS / "logo.png"
TRAY_SOURCE = UI_ASSETS / "logo-tray.svg"
OUT_DIR = ROOT / "src-tauri" / "icons"
TRAY_DIR = OUT_DIR / "tray"
TAURI_CLI = ROOT / "node_modules" / "@tauri-apps" / "cli" / "tauri.js"
SIZES = (16, 24, 32, 48, 64, 128, 256)
TRAY_SIZES = (16, 20, 24, 32, 48)
UI_LOGO_SIZES = (32, 48, 72)
BRAND_500 = (255, 107, 61, 255)
BRAND_50 = (255, 245, 236, 255)


def _oversample_resize(rgba: Image.Image, size: int) -> Image.Image:
    work = max(size * 4, 64)
    step = rgba.resize((work, work), Image.Resampling.LANCZOS)
    return step.resize((size, size), Image.Resampling.LANCZOS)


def _with_status_dot(base: Image.Image) -> Image.Image:
    size = base.width
    scale = 4
    overlay = Image.new("RGBA", (size * scale, size * scale))
    draw = ImageDraw.Draw(overlay)
    diameter = max(4, size // 5)
    margin = max(1, size // 16)
    x = size - diameter - margin
    y = size - diameter - margin
    # 奶油色外圈让暖色状态点与珊瑚色本体分开。
    border = max(1, size // 24)
    draw.ellipse(
        tuple(v * scale for v in (x - border, y - border, x + diameter + border, y + diameter + border)),
        fill=BRAND_50,
    )
    draw.ellipse(
        tuple(v * scale for v in (x, y, x + diameter, y + diameter)),
        fill=BRAND_500,
    )
    return Image.alpha_composite(base, overlay.resize(base.size, Image.Resampling.LANCZOS))


def _desaturate_light(image: Image.Image) -> Image.Image:
    gray = ImageOps.grayscale(image.convert("RGB")).convert("RGBA")
    gray.putalpha(image.getchannel("A"))
    return Image.blend(image, gray, 0.35)


def tauri_icon(source: Path, output: Path, *options: str) -> None:
    subprocess.run(
        ["node", str(TAURI_CLI), "icon", str(source), "--output", str(output), *options],
        check=True,
        cwd=ROOT,
    )


def write_tray_assets(output: Path) -> None:
    # 独立平面稿保留透明背景，小尺寸不带桌面徽章的阴影和留白。
    with tempfile.TemporaryDirectory(prefix="napcat-tray-") as directory:
        temp = Path(directory)
        tauri_icon(TRAY_SOURCE, temp, "--png", "192")
        with Image.open(temp / "192x192.png") as image:
            rgba = image.convert("RGBA")
        output.mkdir(parents=True, exist_ok=True)
        for size in TRAY_SIZES:
            base = _oversample_resize(rgba, size)
            base.save(output / f"tray-{size}.png", optimize=True)
            _with_status_dot(base).save(output / f"tray-active-{size}.png", optimize=True)
            _desaturate_light(base).save(output / f"tray-light-{size}.png", optimize=True)


def write_desktop_assets(rgba: Image.Image, ui_assets: Path, output: Path) -> None:
    for size in UI_LOGO_SIZES:
        _oversample_resize(rgba, size).save(ui_assets / f"logo-{size}.png", optimize=True)
    frames = [_oversample_resize(rgba, size) for size in SIZES]
    # Pillow 按主帧尺寸筛选 ICO 帧，必须以最大帧为主图。
    frames[-1].save(
        output / "icon.ico", format="ICO", sizes=[frame.size for frame in frames],
        append_images=frames[:-1],
    )
    for size in SIZES:
        if size >= 32:
            _oversample_resize(rgba, size).save(output / f"{size}x{size}.png", optimize=True)


def normalize_icns(path: Path) -> None:
    # Tauri 的 ICNS 条目顺序不固定；按类型排序，避免重复生成产生无效 diff。
    data = path.read_bytes()
    if data[:4] != b"icns" or int.from_bytes(data[4:8], "big") != len(data):
        raise ValueError("无效的 ICNS 头")
    entries = []
    offset = 8
    while offset < len(data):
        length = int.from_bytes(data[offset + 4:offset + 8], "big")
        if length < 8 or offset + length > len(data):
            raise ValueError("无效的 ICNS 条目长度")
        entries.append(data[offset:offset + length])
        offset += length
    path.write_bytes(data[:8] + b"".join(sorted(entries, key=lambda entry: entry[:4])))


def publish_assets(staging: Path) -> None:
    changed = 0
    for source in sorted(staging.rglob("*")):
        if not source.is_file():
            continue
        target = ROOT / source.relative_to(staging)
        # Windows 的构建器或预览器可能仍映射着文件，避免原位截断。
        if target.is_file() and target.read_bytes() == source.read_bytes():
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        source.replace(target)
        changed += 1
    print(f"已更新 {changed} 个文件，其余资源内容一致")


def print_plan(source: Path) -> None:
    print(f"主图：{source}")
    if source != SOURCE:
        print(f"导入：{SOURCE}")
    print(f"托盘：{TRAY_SOURCE}")
    print(f"Tauri 平台图标：{OUT_DIR}（PNG / ICO / ICNS / Android / iOS）")
    for size in UI_LOGO_SIZES:
        print(UI_ASSETS / f"logo-{size}.png")
    for size in SIZES:
        if size >= 32:
            print(OUT_DIR / f"{size}x{size}.png")
    for size in TRAY_SIZES:
        for variant in ("tray", "tray-active", "tray-light"):
            print(TRAY_DIR / f"{variant}-{size}.png")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=SOURCE, help="导入已定稿的 1024px RGBA PNG")
    parser.add_argument("--dry-run", action="store_true", help="只检查源图并列出输出范围")
    args = parser.parse_args()
    source = args.source.resolve()
    with Image.open(source) as image:
        if image.format != "PNG" or image.mode != "RGBA" or image.size != (1024, 1024):
            raise SystemExit("主图必须是 1024×1024 的 RGBA PNG")
        rgba = image.copy()
    if not TRAY_SOURCE.is_file():
        raise SystemExit(f"找不到托盘源稿：{TRAY_SOURCE}")
    if not TAURI_CLI.is_file():
        raise SystemExit("请先执行 pnpm install --frozen-lockfile")
    print_plan(source)
    if args.dry_run:
        return
    cache = ROOT / ".cache"
    cache.mkdir(exist_ok=True)
    # 同卷临时目录允许逐文件原子替换，生成失败时不碰正式资源。
    with tempfile.TemporaryDirectory(prefix="app-icons-", dir=cache) as directory:
        staging = Path(directory)
        ui_assets = staging / "src-ui" / "assets"
        output = staging / "src-tauri" / "icons"
        ui_assets.mkdir(parents=True)
        shutil.copyfile(source, ui_assets / "logo.png")
        tauri_icon(ui_assets / "logo.png", output, "--ios-color", "#fff5ec")
        write_desktop_assets(rgba, ui_assets, output)
        write_tray_assets(output / "tray")
        normalize_icns(output / "icon.icns")
        with Image.open(output / "icon.ico") as icon:
            if icon.ico.sizes() != {(size, size) for size in SIZES}:
                raise RuntimeError("ICO 尺寸不完整，未发布生成资源")
        publish_assets(staging)


if __name__ == "__main__":
    main()
