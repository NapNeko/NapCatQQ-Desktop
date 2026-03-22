# -*- coding: utf-8 -*-
"""Desktop 更新规则解析。"""

import re

from src.core.desktop_update.models import DesktopUpdateManifest, DesktopUpdatePlan


def resolve_desktop_update_plan(
    local_version: str | None,
    remote_version: str | None,
    manifest: DesktopUpdateManifest | None,
) -> DesktopUpdatePlan | None:
    """根据本地版本、目标版本和 manifest 解析 Desktop 更新策略。"""

    if not local_version or not remote_version:
        return None

    if _compare_versions(local_version, remote_version) >= 0:
        return None

    if manifest is None:
        return None

    if manifest.min_auto_update_version and _compare_versions(local_version, manifest.min_auto_update_version) < 0:
        return DesktopUpdatePlan(
            kind="unsupported",
            summary="当前版本过旧，已超出自动升级支持范围。",
            min_auto_update_version=manifest.min_auto_update_version,
        )

    for migration in manifest.migrations:
        if migration.matches(local_version, remote_version):
            return DesktopUpdatePlan(kind="migration", summary=migration.summary, migration=migration)

    return None


def _compare_versions(left: str, right: str) -> int:
    """比较两个版本号。"""

    left_parts = _normalize_version(left)
    right_parts = _normalize_version(right)

    for left_part, right_part in zip(left_parts, right_parts):
        if left_part < right_part:
            return -1
        if left_part > right_part:
            return 1

    return 0


def _normalize_version(version: str) -> tuple[int, ...]:
    """将 v1.2.3 这类版本号归一化为整数元组。"""

    match = re.search(r"(\d+(?:\.\d+)*)", version)
    if match is None:
        return (0,)

    parts = tuple(int(part) for part in match.group(1).split("."))
    if len(parts) >= 3:
        return parts

    return parts + (0,) * (3 - len(parts))


def _version_in_range(version: str, min_version: str | None, max_version: str | None) -> bool:
    """判断版本是否处于闭区间范围内。"""

    if min_version and _compare_versions(version, min_version) < 0:
        return False

    if max_version and _compare_versions(version, max_version) > 0:
        return False

    return True
