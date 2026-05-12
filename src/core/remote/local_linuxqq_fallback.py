# -*- coding: utf-8 -*-
"""[`local_linuxqq_fallback`](src/core/remote/local_linuxqq_fallback.py): LinuxQQ
安装包 "本机下载 + SFTP 上传" 兜底.

应用场景: 部分服务器 (典型: 海外 VPS / 出方向受限的网络) 无法直连
``dldir1.qq.com`` (腾讯 CDN), 远端 ``remote_install_linuxqq.sh`` 里
``curl ... dldir1.qq.com/...`` 会卡死或超时. 此时把下载转到 Desktop 本机执行,
再通过 SFTP 上传到 ``${package_dir}/linuxqq_*.deb`` 或 ``*.rpm``;
脚本里 ``verify_qq_package`` + 缓存复用分支会自动跳过下载并复用本地包.

触发策略: 由 [`LinuxCoreDeployment.install_linuxqq`](src/core/remote/deployment.py)
在调用脚本前先做一次远端连通性预检 ([`backend_can_reach_qq_cdn`]); 探测失败才走本机.
对脚本本身**零改动**; 远端能直连 QQ CDN 时行为完全不变.

系统分支: 根据远端探测到的架构 (amd64/arm64) 和包管理器 (dpkg/rpm) 选择对应的
LinuxQQ 安装包 URL 和文件名, 与 ``remote_install_linuxqq.sh`` 中
``select_qq_package`` 函数的逻辑完全对齐.
"""

from __future__ import annotations

from collections.abc import Callable
from pathlib import Path
from typing import Literal

import httpx

from src.core.logging import LogSource, LogType, logger

from .errors import RemoteDeploymentCancelledError
from .execution_backend import ExecutionBackend

LogLineCallback = Callable[[str], None]
"""与 [`LinuxCoreDeployment._run_script_with_progress`](src/core/remote/deployment.py)
共用的"逐行日志透传"协议."""

ShouldCancelCallback = Callable[[], bool]
"""取消检查协议: 返回 ``True`` 表示用户已请求取消."""

# LinuxQQ 版本与 hash 常量 — 与 remote_install_linuxqq.sh 中 select_qq_package 对齐
_LINUXQQ_VERSION: str = "3.2.25-45758"
_LINUXQQ_HASH: str = "7516007c"

# 下载 URL 模板
_QQ_CDN_BASE: str = f"https://dldir1.qq.com/qqfile/qq/QQNT/{_LINUXQQ_HASH}"

# 架构 + 包格式 -> (文件名, 下载 URL)
PackageFormat = Literal["dpkg", "rpm"]
ArchType = Literal["amd64", "arm64"]


def _get_package_info(
    arch: ArchType, pkg_format: PackageFormat
) -> tuple[str, str]:
    """根据架构和包格式返回 (文件名, 下载URL).

    与 ``remote_install_linuxqq.sh`` 中 ``select_qq_package`` 完全对齐.
    """
    if arch == "amd64" and pkg_format == "dpkg":
        filename = f"linuxqq_{_LINUXQQ_VERSION}_amd64.deb"
    elif arch == "amd64" and pkg_format == "rpm":
        filename = f"linuxqq_{_LINUXQQ_VERSION}_x86_64.rpm"
    elif arch == "arm64" and pkg_format == "dpkg":
        filename = f"linuxqq_{_LINUXQQ_VERSION}_arm64.deb"
    elif arch == "arm64" and pkg_format == "rpm":
        filename = f"linuxqq_{_LINUXQQ_VERSION}_aarch64.rpm"
    else:
        raise ValueError(f"不支持的架构/包格式组合: arch={arch}, pkg_format={pkg_format}")

    url = f"{_QQ_CDN_BASE}/{filename}"
    return filename, url


_QQ_CDN_HEALTH_CONNECT_TIMEOUT_SECONDS: int = 5
"""远端 ``curl`` 健康检查的 ``--connect-timeout``."""

_QQ_CDN_HEALTH_MAX_TIMEOUT_SECONDS: int = 8
"""远端 ``curl`` 健康检查的 ``--max-time`` 上限."""


def _raise_if_cancelled(should_cancel: ShouldCancelCallback | None) -> None:
    """取消点检查."""
    if should_cancel is not None and should_cancel():
        raise RemoteDeploymentCancelledError()


def _emit(log_callback: LogLineCallback | None, line: str) -> None:
    """同时落 Desktop 日志与部署控制台 (若有回调)."""
    logger.info(line, LogType.NETWORK, LogSource.CORE)
    if log_callback is not None:
        log_callback(line)


def backend_can_reach_qq_cdn(
    backend: ExecutionBackend,
    *,
    log_callback: LogLineCallback | None = None,
    timeout: int = _QQ_CDN_HEALTH_MAX_TIMEOUT_SECONDS,
    connect_timeout: int = _QQ_CDN_HEALTH_CONNECT_TIMEOUT_SECONDS,
) -> bool:
    """在远端跑一次 ``curl -sI`` 看能否到 QQ CDN (dldir1.qq.com).

    探测目标是 ``dldir1.qq.com`` 的一个实际 LinuxQQ 包 URL (HEAD 请求),
    而不是首页, 确保完整链路可达.

    返回 ``True``: 远端 curl 退出码 0 且 HTTP 状态码以 ``2`` / ``3`` 开头.
    其他情况一律 ``False``, 走本机兜底.
    """
    # 用 amd64 deb 作为探测目标 (最常见的组合)
    probe_url = f"{_QQ_CDN_BASE}/linuxqq_{_LINUXQQ_VERSION}_amd64.deb"
    cmd = (
        f"curl -k -sI -o /dev/null -w '%{{http_code}}' "
        f"--connect-timeout {int(connect_timeout)} --max-time {int(timeout)} "
        f"'{probe_url}' 2>/dev/null || echo 000"
    )
    result = backend.run(cmd, check=False)
    code_lines = (result.stdout or "").strip().splitlines()
    code = code_lines[-1].strip() if code_lines else ""

    reachable = code.startswith(("2", "3"))
    _emit(
        log_callback,
        f"[INFO] 远端 QQ CDN 连通性探测: http_code={code or '000'} "
        f"reachable={'yes' if reachable else 'no'} (probe={probe_url})",
    )
    return reachable


def _download_via_httpx(
    url: str,
    target_path: Path,
    *,
    log_callback: LogLineCallback | None,
    should_cancel: ShouldCancelCallback | None = None,
) -> bool:
    """用 ``httpx.stream`` 把 ``url`` 写到 ``target_path``; 异常视为失败.

    原子写: ``target_path.part`` -> rename.
    """
    partial = target_path.with_name(target_path.name + ".part")
    try:
        target_path.parent.mkdir(parents=True, exist_ok=True)
        if partial.exists():
            partial.unlink()
        with httpx.stream("GET", url, follow_redirects=True, timeout=60) as resp:
            resp.raise_for_status()
            with partial.open("wb") as out:
                for chunk in resp.iter_bytes():
                    _raise_if_cancelled(should_cancel)
                    out.write(chunk)
        partial.replace(target_path)
        return True
    except RemoteDeploymentCancelledError:
        try:
            if partial.exists():
                partial.unlink()
        except OSError:
            pass
        raise
    except (httpx.RequestError, httpx.HTTPStatusError, OSError) as exc:
        _emit(
            log_callback,
            f"[WARN] LinuxQQ 下载失败: {type(exc).__name__}: {exc}",
        )
        try:
            if partial.exists():
                partial.unlink()
        except OSError:
            pass
        return False


def prefetch_linuxqq_package_locally(
    *,
    target_path: Path,
    arch: ArchType,
    pkg_format: PackageFormat,
    log_callback: LogLineCallback | None = None,
    should_cancel: ShouldCancelCallback | None = None,
) -> Path:
    """把 LinuxQQ 安装包下载到本机 ``target_path``.

    根据 ``arch`` 和 ``pkg_format`` 选择正确的包 URL, 与远端脚本
    ``select_qq_package`` 逻辑对齐.

    Args:
        target_path: 本机目标路径 (一般为 ``it(PathFunc).tmp_path / '<filename>'``)
        arch: 远端架构 (``"amd64"`` 或 ``"arm64"``)
        pkg_format: 远端包格式 (``"dpkg"`` 或 ``"rpm"``)
        log_callback: 部署控制台逐行回调
        should_cancel: 取消检查协议

    Returns:
        ``target_path``, 已确认存在.

    Raises:
        RemoteDeploymentCancelledError: 用户中途取消
        RuntimeError: 下载失败
    """
    target_path = Path(target_path)
    filename, url = _get_package_info(arch, pkg_format)

    # 缓存复用: 文件已存在且大小 > 1MB (LinuxQQ 包 ~20MB+, 小于 1MB 一定损坏)
    if target_path.exists():
        file_size = target_path.stat().st_size
        if file_size > 1_048_576:
            _emit(log_callback, f"[INFO] 复用本机 LinuxQQ 缓存: {target_path} ({file_size} bytes)")
            return target_path
        _emit(
            log_callback,
            f"[WARN] 本机 LinuxQQ 缓存文件过小 ({file_size} bytes), 重新下载",
        )
        try:
            target_path.unlink()
        except OSError:
            pass

    _raise_if_cancelled(should_cancel)

    _emit(log_callback, f"[INFO] 本机预下载 LinuxQQ: {filename} (arch={arch}, format={pkg_format})")
    _emit(log_callback, f"[INFO] 下载源: {url}")

    if _download_via_httpx(url, target_path, log_callback=log_callback, should_cancel=should_cancel):
        _emit(log_callback, f"[INFO] LinuxQQ 下载成功: {target_path}")
        return target_path

    raise RuntimeError(
        f"本机预下载 LinuxQQ 失败: {url}"
    )


def get_remote_package_filename(arch: ArchType, pkg_format: PackageFormat) -> str:
    """获取远端 ``${package_dir}`` 下 LinuxQQ 包的文件名.

    必须与 ``remote_install_linuxqq.sh`` 里 ``select_qq_package`` 的
    ``qq_package_path`` 默认值同名, 否则脚本不会复用预上传包.
    """
    filename, _ = _get_package_info(arch, pkg_format)
    return filename


__all__ = (
    "ArchType",
    "LogLineCallback",
    "PackageFormat",
    "ShouldCancelCallback",
    "backend_can_reach_qq_cdn",
    "get_remote_package_filename",
    "prefetch_linuxqq_package_locally",
)
