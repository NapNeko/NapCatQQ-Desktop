# -*- coding: utf-8 -*-
"""[`distro_matrix`](src/core/remote/distro_matrix.py) +
[`LinuxCoreDeploymentProbe.evaluate_compatibility`]
(src/core/remote/deployment.py) 的单元测试.

覆盖:

- KNOWN_DISTROS 自身一致性 (qq_installer 与 family 必须配对)
- ``lookup_distro`` / ``lookup_by_id_like`` 大小写 / 别名 / 未命中分支
- ``list_supported_display_names`` 在三档 tier_floor 下的过滤
- ``evaluate_compatibility`` 五个分支:
  ``supported`` / ``unknown_but_runnable`` / ``unsupported (arch)``
  / ``unsupported (no installer)`` / ``unsupported (mismatch)``
"""
from __future__ import annotations

import pytest

from src.core.remote.deployment import (
    CompatibilityReport,
    LinuxCoreDeploymentProbe,
)
from src.core.remote.distro_matrix import (
    KNOWN_DISTROS,
    list_supported_display_names,
    lookup_by_id_like,
    lookup_distro,
)


# ==================== KNOWN_DISTROS 自身一致性 ====================
def test_known_distros_size() -> None:
    # 至少覆盖 ubuntu / debian / centos / rhel / rocky / alma / fedora 七条
    assert len(KNOWN_DISTROS) >= 7


def test_known_distros_family_installer_consistency() -> None:
    """每条 entry 的 family 必须与 qq_installer 严格配对."""
    for entry in KNOWN_DISTROS:
        if entry.family == "debian":
            assert entry.qq_installer == "dpkg", entry
            assert entry.package_manager == "apt-get", entry
        elif entry.family == "rhel":
            assert entry.qq_installer == "rpm", entry
            assert entry.package_manager in ("dnf", "yum"), entry
        else:  # pragma: no cover - 防御性
            pytest.fail(f"未知 family: {entry.family} on {entry}")


def test_known_distros_no_duplicate_ids() -> None:
    ids = [entry.distro_id for entry in KNOWN_DISTROS]
    assert len(ids) == len(set(ids)), f"重复 distro_id: {ids}"


# ==================== lookup_distro ====================
def test_lookup_distro_exact_hit() -> None:
    assert lookup_distro("ubuntu").family == "debian"
    assert lookup_distro("rocky").qq_installer == "rpm"
    assert lookup_distro("fedora").support_tier == "experimental"


def test_lookup_distro_case_insensitive() -> None:
    assert lookup_distro("UBUNTU") is lookup_distro("ubuntu")
    assert lookup_distro("  RoCky  ").family == "rhel"


def test_lookup_distro_miss() -> None:
    assert lookup_distro("arch") is None
    assert lookup_distro("opensuse-leap") is None
    assert lookup_distro("alpine") is None
    assert lookup_distro(None) is None
    assert lookup_distro("") is None


# ==================== lookup_by_id_like ====================
def test_lookup_by_id_like_rhel_chain() -> None:
    """Rocky 9 ID_LIKE 串里有 rhel/centos/fedora, 应优先命中靠前的 rhel/centos."""
    entry = lookup_by_id_like("rhel centos fedora")
    assert entry is not None
    assert entry.family == "rhel"


def test_lookup_by_id_like_debian_chain() -> None:
    """Linux Mint ID_LIKE='ubuntu debian'."""
    entry = lookup_by_id_like("ubuntu debian")
    assert entry is not None
    assert entry.family == "debian"


def test_lookup_by_id_like_with_quotes() -> None:
    """os-release 里 ID_LIKE 经常带引号, 应被剥掉."""
    entry = lookup_by_id_like('"rhel centos"')
    assert entry is not None
    assert entry.family == "rhel"


def test_lookup_by_id_like_none_or_unknown() -> None:
    assert lookup_by_id_like(None) is None
    assert lookup_by_id_like("") is None
    assert lookup_by_id_like("arch suse") is None


# ==================== list_supported_display_names ====================
def test_display_names_tier_filtering() -> None:
    primary_only = list_supported_display_names(tier_floor="primary")
    assert primary_only == ["Ubuntu"]

    compatible = list_supported_display_names(tier_floor="compatible")
    assert "Ubuntu" in compatible
    assert "Debian" in compatible
    assert "Fedora" not in compatible  # experimental 不应进入 compatible 档

    full = list_supported_display_names(tier_floor="experimental")
    assert "Fedora" in full
    assert len(full) == len(KNOWN_DISTROS)


# ==================== evaluate_compatibility ====================
def _make_probe(**overrides) -> LinuxCoreDeploymentProbe:
    base = dict(
        os_name="Linux",
        architecture="x86_64",
        normalized_arch="amd64",
        distro_id=None,
        distro_version=None,
        has_bash=True,
        has_tar=True,
        has_unzip=True,
        has_curl=True,
        has_dpkg=False,
        has_rpm2cpio=False,
        has_dnf=False,
        has_xvfb=True,
        has_linuxqq=False,
        has_napcat=False,
        installed_qq_version=None,
        installed_napcat_version=None,
        id_like=None,
    )
    base.update(overrides)
    return LinuxCoreDeploymentProbe(**base)


def test_compat_supported_ubuntu_dpkg() -> None:
    probe = _make_probe(distro_id="ubuntu", has_dpkg=True)
    report = probe.evaluate_compatibility()
    assert isinstance(report, CompatibilityReport)
    assert report.compat_status == "supported"
    assert report.family == "debian"
    assert report.distro_entry is not None
    assert report.distro_entry.distro_id == "ubuntu"


def test_compat_supported_rocky_via_id_like() -> None:
    """Rocky 的 distro_id 不在白名单字面量里? 实际上我们已经把 rocky 加进去了,
    所以这里测的是 ID_LIKE 兜底的另一条路: 把 distro_id 设成不在白名单的字符串
    但 ID_LIKE 含 rhel.
    """
    probe = _make_probe(
        distro_id="ol",  # Oracle Linux: 不在 KNOWN_DISTROS
        id_like="rhel fedora",
        has_rpm2cpio=True,
    )
    report = probe.evaluate_compatibility()
    assert report.compat_status == "supported"
    assert report.family == "rhel"


def test_compat_unsupported_arch() -> None:
    probe = _make_probe(architecture="riscv64", normalized_arch=None, has_dpkg=True)
    report = probe.evaluate_compatibility()
    assert report.compat_status == "unsupported"
    assert any("CPU 架构" in r for r in report.reasons)


def test_compat_unsupported_no_installer() -> None:
    probe = _make_probe(distro_id="ubuntu")  # has_dpkg / has_rpm2cpio 都 False
    report = probe.evaluate_compatibility()
    assert report.compat_status == "unsupported"
    assert any("解包工具" in r for r in report.reasons)


def test_compat_unsupported_installer_mismatch() -> None:
    """声称 centos 但远端只有 dpkg, 没有 rpm2cpio."""
    probe = _make_probe(distro_id="centos", has_dpkg=True, has_rpm2cpio=False)
    report = probe.evaluate_compatibility()
    assert report.compat_status == "unsupported"
    assert report.distro_entry is not None
    assert report.distro_entry.distro_id == "centos"
    assert any("rpm2cpio" in r for r in report.reasons)


def test_compat_unknown_but_runnable() -> None:
    """Arch Linux: 未识别但探测到 dpkg (假设用户装了), 应返回 unknown_but_runnable."""
    probe = _make_probe(distro_id="arch", has_dpkg=True)
    report = probe.evaluate_compatibility()
    assert report.compat_status == "unknown_but_runnable"
    assert report.distro_entry is None
    assert report.family is None
    assert any("未识别" in r for r in report.reasons)


def test_compat_unknown_distro_id_none_with_installer() -> None:
    """``/etc/os-release`` 缺失但有 dpkg -> unknown_but_runnable."""
    probe = _make_probe(distro_id=None, id_like=None, has_dpkg=True)
    report = probe.evaluate_compatibility()
    assert report.compat_status == "unknown_but_runnable"
