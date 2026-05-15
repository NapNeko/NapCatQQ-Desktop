# -*- coding: utf-8 -*-
"""[`local_node_fallback`](src/core/remote/snowluma/local_node_fallback.py): Node.js
便携式 tarball "本机下载 + SFTP 上传" 兜底.

应用场景: SnowLuma 远端部署的 ``install_snowluma.sh`` L4 阶段需要下载
``node-v22.x-linux-{x64|arm64}.tar.xz`` (~30MB), 但部分服务器 (典型:
海外 VPS / 出方向受限的网络) 无法直连 npmmirror / nodejs.org 等镜像站,
导致 L4 所有 6 个镜像源都超时, 最终 ``die "NODE_VERSION_TOO_LOW"``.

此时把下载转到 Desktop 本机执行 (本机大概率有更好的网络), 再通过 SFTP
上传到 ``${workspace_dir}/packages/node-vX.Y.Z-linux-{arch}.tar.xz``;
脚本里新增的 ``NODE_PRELOADED`` 分支会自动跳过网络下载并复用预上传包.

触发策略: 由 [`SnowLumaDeployment.install_snowluma_framework`] 在调用脚本前
先做一次远端连通性预检 ([`backend_can_reach_node_mirrors`]); 探测失败才走本机.
远端能直连镜像站时该路径不会触发, 行为完全不变.

系统分支: 根据远端架构 (amd64/arm64) 选择对应的 tarball 文件名, 与
``install_snowluma.sh.j2`` 中 L4 的 ``NODE_ARCH`` / ``NODE_TARBALL`` 对齐.
"""

from __future__ import annotations

from collections.abc import Callable
from pathlib import Path
from typing import Literal

import httpx

from src.core.logging import LogSource, LogType, logger

from ..errors import RemoteDeploymentCancelledError
from ..execution_backend import ExecutionBackend

LogLineCallback = Callable[[str], None]
ShouldCancelCallback = Callable[[], bool]

# Node.js 版本 - 与 install_snowluma.sh.j2 L4 的 NODE_VERSION_TAG 对齐
_NODE_VERSION_TAG: str = "v22.18.0"

# 架构映射 - 与脚本中 case "$(uname -m)" 对齐
ArchType = Literal["amd64", "arm64"]

# 镜像源列表 - 与脚本中 L4 的 for url in ... 对齐 (顺序一致)
_NODE_MIRROR_URLS: list[str] = [
    f"https://cdn.npmmirror.com/binaries/node/{_NODE_VERSION_TAG}",
    f"https://npmmirror.com/mirrors/node/{_NODE_VERSION_TAG}",
    f"https://mirrors.huaweicloud.com/nodejs/{_NODE_VERSION_TAG}",
    f"https://mirrors.cloud.tencent.com/nodejs-release/{_NODE_VERSION_TAG}",
    f"https://registry.npmmirror.com/-/binary/node/{_NODE_VERSION_TAG}",
    f"https://nodejs.org/dist/{_NODE_VERSION_TAG}",
]

_HEALTH_CONNECT_TIMEOUT_SECONDS: int = 5
_HEALTH_MAX_TIMEOUT_SECONDS: int = 8


def _get_node_tarball_filename(arch: ArchType) -> str:
    """根据架构返回 tarball 文件名.

    与 ``install_snowluma.sh.j2`` L4 的 ``NODE_TARBALL`` 变量对齐.
    """
    node_arch = "x64" if arch == "amd64" else "arm64"
    return f"node-{_NODE_VERSION_TAG}-linux-{node_arch}.tar.xz"


def _raise_if_cancelled(should_cancel: ShouldCancelCallback | None) -> None:
    if should_cancel is not None and should_cancel():
        raise RemoteDeploymentCancelledError()


def _emit(log_callback: LogLineCallback | None, line: str) -> None:
    logger.info(line, LogType.NETWORK, LogSource.CORE)
    if log_callback is not None:
        log_callback(line)


def backend_can_reach_node_mirrors(
    backend: ExecutionBackend,
    *,
    arch: ArchType = "amd64",
    log_callback: LogLineCallback | None = None,
    timeout: int = _HEALTH_MAX_TIMEOUT_SECONDS,
    connect_timeout: int = _HEALTH_CONNECT_TIMEOUT_SECONDS,
) -> bool:
    """在远端跑一次 ``curl -sI`` 看能否到 Node.js 镜像站.

    按脚本中镜像优先级, 探测第一个源 (npmmirror CDN). 只要有一个源可达即返回 True.

    返回 ``True``: 远端 curl 退出码 0 且 HTTP 状态码以 ``2`` / ``3`` 开头.
    其他情况一律 ``False``, 走本机兜底.
    """
    filename = _get_node_tarball_filename(arch)
    # 探测前两个镜像 (npmmirror CDN + npmmirror mirrors), 任一可达即 OK
    probe_urls = [f"{base}/{filename}" for base in _NODE_MIRROR_URLS[:2]]

    for probe_url in probe_urls:
        cmd = (
            f"curl -k -sI -o /dev/null -w '%{{http_code}}' "
            f"--connect-timeout {int(connect_timeout)} --max-time {int(timeout)} "
            f"'{probe_url}' 2>/dev/null || echo 000"
        )
        result = backend.run(cmd, check=False)
        code_lines = (result.stdout or "").strip().splitlines()
        code = code_lines[-1].strip() if code_lines else ""

        if code.startswith(("2", "3")):
            _emit(
                log_callback,
                f"[INFO] 远端 Node 镜像连通性探测: http_code={code} "
                f"reachable=yes (probe={probe_url})",
            )
            return True

    _emit(
        log_callback,
        "[INFO] 远端 Node 镜像连通性探测: reachable=no (所有探测源均不可达)",
    )
    return False


def _download_via_httpx(
    url: str,
    target_path: Path,
    *,
    log_callback: LogLineCallback | None,
    should_cancel: ShouldCancelCallback | None = None,
) -> bool:
    """用 ``httpx.stream`` 把 ``url`` 写到 ``target_path``; 异常视为失败."""
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
            f"[WARN] Node tarball 源 {url} 下载失败: {type(exc).__name__}: {exc}",
        )
        try:
            if partial.exists():
                partial.unlink()
        except OSError:
            pass
        return False


def prefetch_node_tarball_locally(
    *,
    target_path: Path,
    arch: ArchType,
    log_callback: LogLineCallback | None = None,
    should_cancel: ShouldCancelCallback | None = None,
) -> Path:
    """把 Node.js 便携式 tarball 下载到本机 ``target_path``.

    顺序尝试所有镜像源 (与脚本 L4 对齐), 直至一源成功.
    已存在且大小 > 5MB 的本机缓存直接复用.

    Args:
        target_path: 本机目标路径
        arch: 远端架构 (``"amd64"`` 或 ``"arm64"``)
        log_callback: 部署控制台逐行回调
        should_cancel: 取消检查协议

    Returns:
        ``target_path``, 已确认存在.

    Raises:
        RemoteDeploymentCancelledError: 用户中途取消
        RuntimeError: 全部候选源都失败
    """
    target_path = Path(target_path)
    filename = _get_node_tarball_filename(arch)

    # 缓存复用: node tarball ~30MB, 5MB 作为保守阈值
    if target_path.exists():
        file_size = target_path.stat().st_size
        if file_size > 5_242_880:
            _emit(log_callback, f"[INFO] 复用本机 Node tarball 缓存: {target_path} ({file_size} bytes)")
            return target_path
        _emit(
            log_callback,
            f"[WARN] 本机 Node tarball 缓存过小 ({file_size} bytes), 重新下载",
        )
        try:
            target_path.unlink()
        except OSError:
            pass

    _raise_if_cancelled(should_cancel)

    candidates = [f"{base}/{filename}" for base in _NODE_MIRROR_URLS]
    _emit(log_callback, f"[INFO] 本机预下载 Node tarball: {filename} (arch={arch}), 候选源数={len(candidates)}")

    for idx, url in enumerate(candidates, 1):
        _raise_if_cancelled(should_cancel)
        _emit(log_callback, f"[INFO] [{idx}/{len(candidates)}] 尝试: {url}")
        if _download_via_httpx(url, target_path, log_callback=log_callback, should_cancel=should_cancel):
            _emit(log_callback, f"[INFO] Node tarball 下载成功 (源 {idx}/{len(candidates)}): {target_path}")
            return target_path

    raise RuntimeError(
        f"本机预下载 Node tarball 失败: 已尝试 {len(candidates)} 个源都不可用"
    )


def get_remote_node_tarball_filename(arch: ArchType) -> str:
    """获取远端 ``${workspace_dir}/packages/`` 下 Node tarball 的文件名.

    必须与 ``install_snowluma.sh.j2`` L4 的 ``NODE_PRELOADED`` 路径中的
    文件名对齐, 否则脚本不会复用预上传包.
    """
    return _get_node_tarball_filename(arch)


__all__ = (
    "ArchType",
    "LogLineCallback",
    "ShouldCancelCallback",
    "backend_can_reach_node_mirrors",
    "get_remote_node_tarball_filename",
    "prefetch_node_tarball_locally",
)
