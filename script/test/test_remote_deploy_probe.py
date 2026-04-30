# -*- coding: utf-8 -*-
"""[`LinuxCoreDeployment.probe_environment`](src/core/remote/deployment.py) 单元测试。"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path

import pytest

from src.core.remote.deployment import LinuxCoreDeployment
from src.core.remote.execution_backend import ExecutionBackend
from src.core.remote.models import LinuxCorePaths, RemoteCommandResult


@dataclass
class FakeExecutionBackend(ExecutionBackend):
    """根据“命令 -> 结果”映射表伪造命令执行的测试后端。

    匹配优先级:
        1. 命令完全相等
        2. 命令包含某个映射键(子串)
        3. 默认: 退出码 0, 空 stdout

    通过传入 ``responder`` 可对未命中映射的命令返回自定义结果。
    """

    fixed: dict[str, RemoteCommandResult] = field(default_factory=dict)
    contains: list[tuple[str, RemoteCommandResult]] = field(default_factory=list)
    default_factory: Callable[[str], RemoteCommandResult] | None = None
    history: list[str] = field(default_factory=list)
    upload_calls: list[tuple[str, str]] = field(default_factory=list)
    ensure_dir_calls: list[str] = field(default_factory=list)

    def run(self, command: str, *, timeout: float | None = None, check: bool = False) -> RemoteCommandResult:
        self.history.append(command)
        if command in self.fixed:
            return self.fixed[command]
        for needle, result in self.contains:
            if needle in command:
                return result
        if self.default_factory is not None:
            return self.default_factory(command)
        return RemoteCommandResult(command=command, exit_status=0, stdout="", stderr="")

    def ensure_directory(self, path: str) -> RemoteCommandResult:
        self.ensure_dir_calls.append(path)
        return RemoteCommandResult(command=f"mkdir -p {path}", exit_status=0)

    def upload_file(self, local_path, target_path: str) -> None:
        self.upload_calls.append((str(local_path), target_path))

    def download_file(self, source_path: str, local_path) -> None:  # pragma: no cover - 探测用例不调用
        pass


def _build_probe_backend(
    *,
    os_name: str = "Linux",
    arch: str = "x86_64",
    os_release: str = 'ID=ubuntu\nVERSION_ID="22.04"\n',
    has_bash: bool = True,
    has_tar: bool = True,
    has_unzip: bool = True,
    has_curl: bool = True,
    has_dpkg: bool = True,
    has_rpm2cpio: bool = False,
    has_xvfb: bool = True,
    qq_present: bool = False,
    qq_pkg_json: str = "",
    napcat_present: bool = False,
    napcat_mjs: str = "",
    paths: LinuxCorePaths | None = None,
) -> FakeExecutionBackend:
    paths = paths or LinuxCorePaths()
    fixed: dict[str, RemoteCommandResult] = {
        "uname -s": RemoteCommandResult(command="uname -s", exit_status=0, stdout=os_name + "\n"),
        "uname -m": RemoteCommandResult(command="uname -m", exit_status=0, stdout=arch + "\n"),
        "test -f /etc/os-release && cat /etc/os-release || true": RemoteCommandResult(
            command="cat /etc/os-release", exit_status=0, stdout=os_release
        ),
        "command -v bash >/dev/null 2>&1": RemoteCommandResult(command="-", exit_status=0 if has_bash else 1),
        "command -v tar >/dev/null 2>&1": RemoteCommandResult(command="-", exit_status=0 if has_tar else 1),
        "command -v unzip >/dev/null 2>&1": RemoteCommandResult(command="-", exit_status=0 if has_unzip else 1),
        "command -v curl >/dev/null 2>&1": RemoteCommandResult(command="-", exit_status=0 if has_curl else 1),
        "command -v dpkg >/dev/null 2>&1": RemoteCommandResult(command="-", exit_status=0 if has_dpkg else 1),
        "command -v rpm2cpio >/dev/null 2>&1 && command -v cpio >/dev/null 2>&1": RemoteCommandResult(
            command="-", exit_status=0 if has_rpm2cpio else 1
        ),
        "command -v xvfb-run >/dev/null 2>&1": RemoteCommandResult(command="-", exit_status=0 if has_xvfb else 1),
    }

    qq_check_cmd = (
        f'test -x "{paths.qq_executable}" && test -f "{paths.qq_package_json_path}" '
        f"&& echo yes || echo no"
    )
    fixed[qq_check_cmd] = RemoteCommandResult(command="-", exit_status=0, stdout="yes\n" if qq_present else "no\n")

    qq_cat_cmd = f'cat "{paths.qq_package_json_path}" 2>/dev/null || true'
    fixed[qq_cat_cmd] = RemoteCommandResult(command="-", exit_status=0, stdout=qq_pkg_json)

    napcat_check_cmd = f'test -f "{paths.napcat_dir}/napcat.mjs" && echo yes || echo no'
    fixed[napcat_check_cmd] = RemoteCommandResult(
        command="-", exit_status=0, stdout="yes\n" if napcat_present else "no\n"
    )

    # napcat 版本探测的 grep 用 contains 匹配, 避免和实际命令字符串紧耦合
    # (实际命令: ``grep -oE 'napCatVersion[^;]*' "...napcat.mjs" ... | head -n1``)
    contains: list[tuple[str, RemoteCommandResult]] = [
        (
            "grep -oE 'napCatVersion[^;]*'",
            RemoteCommandResult(command="-", exit_status=0, stdout=napcat_mjs),
        ),
    ]

    return FakeExecutionBackend(fixed=fixed, contains=contains)


class TestProbeEnvironment:
    def test_full_ubuntu_amd64_with_existing_install(self) -> None:
        backend = _build_probe_backend(
            arch="x86_64",
            qq_present=True,
            qq_pkg_json='{"version": "9.9.9-test", "name": "qq"}',
            napcat_present=True,
            # 简化版 napCatVersion 字段, 用于通用回归
            napcat_mjs='const napCatVersion = "0.5.7";',
        )
        deployment = LinuxCoreDeployment(backend)

        probe = deployment.probe_environment()

        assert probe.os_name == "Linux"
        assert probe.architecture == "x86_64"
        assert probe.normalized_arch == "amd64"
        assert probe.distro_id == "ubuntu"
        assert probe.distro_version == "22.04"
        assert probe.has_bash and probe.has_tar and probe.has_unzip and probe.has_curl
        assert probe.has_dpkg
        assert not probe.has_rpm2cpio
        assert probe.has_xvfb
        assert probe.has_linuxqq is True
        assert probe.has_napcat is True
        assert probe.installed_qq_version == "9.9.9-test"
        assert probe.installed_napcat_version == "0.5.7"
        assert probe.is_supported_arch is True
        assert probe.has_package_installer is True

    def test_arm64_normalization(self) -> None:
        backend = _build_probe_backend(arch="aarch64")
        probe = LinuxCoreDeployment(backend).probe_environment()
        assert probe.architecture == "aarch64"
        assert probe.normalized_arch == "arm64"
        assert probe.is_supported_arch is True

    def test_unsupported_arch_returns_none(self) -> None:
        backend = _build_probe_backend(arch="riscv64")
        probe = LinuxCoreDeployment(backend).probe_environment()
        assert probe.normalized_arch is None
        assert probe.is_supported_arch is False

    def test_missing_os_release(self) -> None:
        backend = _build_probe_backend(os_release="")
        probe = LinuxCoreDeployment(backend).probe_environment()
        assert probe.distro_id is None
        assert probe.distro_version is None

    def test_centos_rpm_only(self) -> None:
        backend = _build_probe_backend(
            os_release='ID="centos"\nVERSION_ID="7"\n',
            has_dpkg=False,
            has_rpm2cpio=True,
        )
        probe = LinuxCoreDeployment(backend).probe_environment()
        assert probe.distro_id == "centos"
        assert probe.distro_version == "7"
        assert probe.has_dpkg is False
        assert probe.has_rpm2cpio is True
        assert probe.has_package_installer is True

    def test_missing_napcat_mjs_means_no_napcat(self) -> None:
        backend = _build_probe_backend(napcat_present=False)
        probe = LinuxCoreDeployment(backend).probe_environment()
        assert probe.has_napcat is False
        assert probe.installed_napcat_version is None

    def test_existing_napcat_with_unparseable_mjs(self) -> None:
        backend = _build_probe_backend(
            napcat_present=True,
            napcat_mjs="// 没有 napCatVersion 字段",
        )
        probe = LinuxCoreDeployment(backend).probe_environment()
        assert probe.has_napcat is True
        assert probe.installed_napcat_version is None

    def test_real_napcat_shell_zip_mjs_format(self) -> None:
        """**关键回归**: 真实 NapCat.Shell.zip 中 ``napcat.mjs`` 的版本字段形态。

        正则必须能跨越 ``"undefined"`` 引号字符串字面量, 找到真正的版本号 ``"4.18.1"``。
        历史上用 ``[^"]*`` 会卡在 ``"undefined"`` 上无法匹配, 这条用例就是为了
        防止未来再退步到非贪婪正则丢失。
        """
        # 这一行 byte-for-byte 来自 NapCat.Shell.zip latest 解出的 napcat.mjs:10375
        real_mjs_line = (
            'const napCatVersion = typeof (__vite_import_meta_env__) '
            '!== "undefined" && "4.18.1" || "1.0.0-dev";'
        )
        backend = _build_probe_backend(
            napcat_present=True,
            napcat_mjs=real_mjs_line,
        )
        probe = LinuxCoreDeployment(backend).probe_environment()
        assert probe.has_napcat is True
        assert probe.installed_napcat_version == "4.18.1", (
            "正则必须用 .*? 非贪婪以跨越 \"undefined\" 抓到 \"4.18.1\""
        )

    def test_existing_qq_without_version_field(self) -> None:
        backend = _build_probe_backend(
            qq_present=True,
            qq_pkg_json='{"name": "qq", "no": "version"}',
        )
        probe = LinuxCoreDeployment(backend).probe_environment()
        assert probe.has_linuxqq is True
        assert probe.installed_qq_version is None

    def test_parse_os_release_handles_quotes_and_comments(self) -> None:
        sample = (
            "# this is a comment\n"
            'NAME="My Distro"\n'
            "ID=alpine\n"
            "VERSION_ID='3.18'\n"
        )
        distro_id, version = LinuxCoreDeployment._parse_os_release(sample)
        assert distro_id == "alpine"
        assert version == "3.18"
