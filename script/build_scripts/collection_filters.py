# -*- coding: utf-8 -*-
"""PyInstaller collection filters for reducing bundle size."""

# 标准库导入
from __future__ import annotations

from typing import Iterable


def _normalize_dest(path: str) -> str:
    return path.replace("\\", "/")


def _rebuild(entries: Iterable[tuple[str, str, str]], filtered: list[tuple[str, str, str]]):
    try:
        return entries.__class__(filtered)
    except Exception:
        return filtered


def _filter_qt_translations(entries, locales: tuple[str, ...]) -> tuple[object, int]:
    kept: list[tuple[str, str, str]] = []
    removed = 0
    suffixes = tuple(f"_{locale}.qm" for locale in locales)

    for entry in entries:
        dest, src, typecode = entry
        normalized_dest = _normalize_dest(dest)
        if normalized_dest.startswith("PySide6/translations/") and not normalized_dest.endswith(suffixes):
            removed += 1
            continue
        kept.append((dest, src, typecode))

    return _rebuild(entries, kept), removed


def _filter_pillow_avif_modules(entries) -> tuple[object, int]:
    kept: list[tuple[str, str, str]] = []
    removed = 0

    for entry in entries:
        dest, src, typecode = entry
        normalized_dest = _normalize_dest(dest)
        normalized_src = _normalize_dest(src)

        if normalized_dest == "PIL.AvifImagePlugin" or normalized_src.endswith("/PIL/AvifImagePlugin.py"):
            removed += 1
            continue

        if normalized_dest.startswith("PIL/_avif.") or "/PIL/_avif." in normalized_src:
            removed += 1
            continue

        kept.append((dest, src, typecode))

    return _rebuild(entries, kept), removed


def filter_analysis_collections(analysis, locales: tuple[str, ...] = ("zh_CN",)) -> None:
    """Prune optional resources from the PyInstaller analysis graph."""

    analysis.datas, removed_translations = _filter_qt_translations(analysis.datas, locales)
    analysis.pure, removed_avif_modules = _filter_pillow_avif_modules(analysis.pure)
    analysis.binaries, removed_avif_binaries = _filter_pillow_avif_modules(analysis.binaries)

    print(
        "[build_filters] kept Qt translations for",
        ",".join(locales),
        f"| removed translations={removed_translations}",
        f"| removed pillow avif modules={removed_avif_modules}",
        f"| removed pillow avif binaries={removed_avif_binaries}",
    )
