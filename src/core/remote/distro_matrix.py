# -*- coding: utf-8 -*-
"""[`distro_matrix`](src/core/remote/distro_matrix.py): 远端 Linux 发行版集中数据模块.

本模块把"哪个发行版属于哪个家族 / 该用什么包管理器 / LinuxQQ 该取 deb 还是 rpm"
从远端 shell 脚本里抽到 Python 侧, 给上层 [`LinuxCoreDeploymentProbe`]
(src/core/remote/deployment.py) 的兼容性评估提供静态查表数据.

设计要点
--------

- **只**描述静态映射, 不持有运行期状态; 所有 entry 都是 ``frozen=True`` 的 dataclass
- **不**直接负责依赖包名翻译 (那是脚本侧 ``install_missing_dependencies`` 的职责);
  本模块只回答 "这个发行版用 apt-get 还是 dnf" / "QQ 包要 dpkg 还是 rpm" 这种二选一
- 通过 ``ID_LIKE`` 兜底 (``lookup_by_id_like``), 让派生发行版 (Linux Mint -> Debian,
  Oracle Linux -> RHEL) 也能落到正确家族而不必单独加 entry

支持等级 (``support_tier``)
--------------------------

- ``primary``: 项目主力实测发行版 (Ubuntu)
- ``compatible``: 已通过分发逻辑覆盖测试, 但缺乏真实环境实测
- ``experimental``: 已知能跑但发行版本身节奏快 / 库版本变更频繁 (Fedora)
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal

DistroFamily = Literal["debian", "rhel"]
PackageManager = Literal["apt-get", "dnf", "yum"]
QQInstaller = Literal["dpkg", "rpm"]
SupportTier = Literal["primary", "compatible", "experimental"]


@dataclass(frozen=True, slots=True)
class DistroEntry:
    """单条发行版描述."""

    distro_id: str
    family: DistroFamily
    package_manager: PackageManager
    qq_installer: QQInstaller
    display_name: str
    support_tier: SupportTier


# 注意: 顺序敏感 - lookup_by_id_like 按顺序查找,
# 把更通用的家族标识 (debian/rhel) 放在派生项前确保 ID_LIKE 兜底落到正确家族.
KNOWN_DISTROS: tuple[DistroEntry, ...] = (
    DistroEntry("ubuntu", "debian", "apt-get", "dpkg", "Ubuntu", "primary"),
    DistroEntry("debian", "debian", "apt-get", "dpkg", "Debian", "compatible"),
    DistroEntry("centos", "rhel", "dnf", "rpm", "CentOS / CentOS Stream", "compatible"),
    DistroEntry("rhel", "rhel", "dnf", "rpm", "Red Hat Enterprise Linux", "compatible"),
    DistroEntry("rocky", "rhel", "dnf", "rpm", "Rocky Linux", "compatible"),
    DistroEntry("almalinux", "rhel", "dnf", "rpm", "AlmaLinux", "compatible"),
    DistroEntry("fedora", "rhel", "dnf", "rpm", "Fedora", "experimental"),
)


def lookup_distro(distro_id: str | None) -> DistroEntry | None:
    """按 ``/etc/os-release`` ID 精确查表; 未命中或 ``None`` 时返回 ``None``."""
    if not distro_id:
        return None
    target = distro_id.strip().lower()
    for entry in KNOWN_DISTROS:
        if entry.distro_id == target:
            return entry
    return None


def lookup_by_id_like(id_like: str | None) -> DistroEntry | None:
    """按 ``/etc/os-release`` ID_LIKE 兜底查表.

    ID_LIKE 字段是空格分隔的"父发行版"标识列表, 例如:

    - Linux Mint -> ``ID_LIKE="ubuntu debian"``
    - Rocky 9   -> ``ID_LIKE="rhel centos fedora"``
    - Oracle 9  -> ``ID_LIKE="rhel fedora"``

    本函数返回 **第一个能在** ``KNOWN_DISTROS`` **里命中的** entry, 用作家族归属推断.
    若所有 token 都未命中则返回 ``None``.
    """
    if not id_like:
        return None
    tokens = [tok.strip().lower() for tok in id_like.replace('"', "").split() if tok.strip()]
    for token in tokens:
        entry = lookup_distro(token)
        if entry is not None:
            return entry
    return None


def list_supported_display_names(*, tier_floor: SupportTier = "experimental") -> list[str]:
    """按 ``support_tier`` 阈值返回展示名列表, 用于 UI 文案动态拼接.

    Args:
        tier_floor: 最低含入等级; ``"primary"`` 仅含 Ubuntu, ``"experimental"`` 含全部.
    """
    order: tuple[SupportTier, ...] = ("primary", "compatible", "experimental")
    floor_idx = order.index(tier_floor)
    return [
        entry.display_name
        for entry in KNOWN_DISTROS
        if order.index(entry.support_tier) <= floor_idx
    ]


__all__: tuple[str, ...] = (
    "DistroEntry",
    "DistroFamily",
    "PackageManager",
    "QQInstaller",
    "SupportTier",
    "KNOWN_DISTROS",
    "lookup_distro",
    "lookup_by_id_like",
    "list_supported_display_names",
)
