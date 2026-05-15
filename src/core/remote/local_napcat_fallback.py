# -*- coding: utf-8 -*-
"""[`local_napcat_fallback`](src/core/remote/local_napcat_fallback.py): NapCat
归档 "本机下载 + SFTP 上传" 兜底.

应用场景: 部分服务器 (典型: 国内云厂商 + 出方向受限的网络) 无法直连
``github.com`` / ``objects.githubusercontent.com``, 远端 ``remote_install_napcat.sh``
里 ``curl ... github.com/...`` 会卡死或 4xx. 此时把下载转到 Desktop 本机执行
(本机大概率有镜像站可用), 再通过 SFTP 上传到 ``${package_dir}/NapCat.Shell.zip``;
脚本里 ``[ -f "$napcat_archive_path" ]`` 分支会自动跳过下载并复用本地包.

触发策略: 由 [`LinuxCoreDeployment.install_napcat`](src/core/remote/deployment.py)
在调用脚本前先做一次远端连通性预检 ([`backend_can_reach_github`]); 探测失败才走本机.
对脚本本身**零改动**; 远端能直连 GitHub 时行为完全不变.

SHA512 校验与 [`verify_napcat_archive`](src/core/installation/installers.py) 同语义,
命中 ``expected_sha512`` 后才上传, 防止把损坏归档推到远端.
"""

from __future__ import annotations

import hashlib
from collections.abc import Callable
from pathlib import Path

import httpx

from src.core.logging import LogSource, LogType, logger
from src.core.network.urls import Urls

from .errors import RemoteDeploymentCancelledError
from .execution_backend import ExecutionBackend

LogLineCallback = Callable[[str], None]
"""与 [`LinuxCoreDeployment._run_script_with_progress`](src/core/remote/deployment.py)
共用的"逐行日志透传"协议."""

ShouldCancelCallback = Callable[[], bool]
"""取消检查协议: 返回 ``True`` 表示用户已请求取消; 调用方会招 :class:`RemoteDeploymentCancelledError`.
一般是 [`threading.Event.is_set`](https://docs.python.org/3/library/threading.html#threading.Event)
的绑定方法引用 (由 ServerManager 提供)."""


def _raise_if_cancelled(should_cancel: ShouldCancelCallback | None) -> None:
    """取消点检查: ``should_cancel()`` 返回 True 时招 :class:`RemoteDeploymentCancelledError`.

    `None` 等价于"不检查", 兼容不提供取消能力的调用者 (测试 / 旧版本主调)."""
    if should_cancel is not None and should_cancel():
        raise RemoteDeploymentCancelledError()

_HASH_CHUNK_SIZE: int = 1024 * 1024  # 1 MiB

_GITHUB_HEALTH_CONNECT_TIMEOUT_SECONDS: int = 5
"""远端 ``curl`` 健康检查的 ``--connect-timeout``.
显式区分"连接 (TCP+TLS) 阶段"和"整体超时", 避免在 TLS 握手抖动的网络上靠
``--max-time`` 兜底导致探测严重慢于实际下载就失败的判定."""

_GITHUB_HEALTH_MAX_TIMEOUT_SECONDS: int = 8
"""远端 ``curl`` 健康检查的 ``--max-time`` 上限.
覆盖 ``github.com -> objects.githubusercontent.com`` 的 302 + 二次 TLS 握手 + HEAD 响应;
配合 ``--connect-timeout`` 让网络抖动场景下探测**严于而非松于**远端脚本的真实下载
(``remote_install_napcat.sh`` 用的是 ``--connect-timeout 8``), 杜绝"探测过, 下载挂"假阳性."""


def _compute_sha512(path: Path) -> str:
    """流式计算 ``path`` 的 SHA512 hex digest."""
    hasher = hashlib.sha512()
    with path.open("rb") as fp:
        while True:
            chunk = fp.read(_HASH_CHUNK_SIZE)
            if not chunk:
                break
            hasher.update(chunk)
    return hasher.hexdigest()


def _emit(log_callback: LogLineCallback | None, line: str) -> None:
    """同时落 Desktop 日志与部署控制台 (若有回调)."""
    logger.info(line, LogType.NETWORK, LogSource.CORE)
    if log_callback is not None:
        log_callback(line)


def backend_can_reach_github(
    backend: ExecutionBackend,
    *,
    log_callback: LogLineCallback | None = None,
    timeout: int = _GITHUB_HEALTH_MAX_TIMEOUT_SECONDS,
    connect_timeout: int = _GITHUB_HEALTH_CONNECT_TIMEOUT_SECONDS,
) -> bool:
    """在远端跑一次 ``curl -sIL`` 看能否到 NapCat 发布资产.

    探测目标是 [`Urls.NAPCATQQ_DOWNLOAD`](src/core/network/urls.py)
    (``https://github.com/.../releases/latest/download/NapCat.Shell.zip``)
    而**不是** ``github.com`` 首页, 因为:

    * ``github.com`` 首页常被边缘节点缓存, 即使到 release 资产的链路完全不通也能拿到 200,
      让 Desktop 误判"远端能直连", 跳过本机下载 + SFTP 上传兜底, 最终远端脚本下载阶段挂掉.
    * 真实下载会经历 ``github.com -> objects.githubusercontent.com`` 的 302 + 二次 TLS 握手,
      只有把这条完整路径打通才算"远端能连". HEAD + ``-L`` 让探测严格执行同一链路而不下载 100MB.
    * ``-k --connect-timeout 5 --max-time 8`` 与远端 [`remote_install_napcat.sh`]
      (src/resource/script/remote_install_napcat.sh) 的 ``_try_download``
      (``-k --connect-timeout 8``) 对齐, 探测略**严于**真实下载, 把"探测过/下载挂"的
      假阳性窗口压到最小.

    返回 ``True``: 远端 curl 退出码 0 且 HTTP 状态码以 ``2`` / ``3`` 开头.
    其他情况 (含 curl 不存在 / DNS 失败 / 超时 / 4xx / 5xx) 一律 ``False``,
    走本机兜底, 让部署"宁可慢一点也不卡死".

    Args:
        timeout: ``--max-time`` 总超时秒数 (覆盖整个 HEAD + 重定向 + HEAD 链).
        connect_timeout: ``--connect-timeout`` TCP+TLS 握手超时秒数; 与 ``timeout``
            共同决定探测的严格度. 应保持 ``connect_timeout <= timeout``.
    """
    probe_url = Urls.NAPCATQQ_DOWNLOAD.value.toString()
    cmd = (
        f"curl -k -sIL -o /dev/null -w '%{{http_code}}' "
        f"--connect-timeout {int(connect_timeout)} --max-time {int(timeout)} "
        f"{probe_url} 2>/dev/null || echo 000"
    )
    result = backend.run(cmd, check=False)
    code_lines = (result.stdout or "").strip().splitlines()
    code = code_lines[-1].strip() if code_lines else ""

    reachable = code.startswith(("2", "3"))
    _emit(
        log_callback,
        f"[INFO] 远端 GitHub 连通性探测: http_code={code or '000'} "
        f"reachable={'yes' if reachable else 'no'} (probe={probe_url})",
    )
    return reachable


def _build_candidate_urls() -> list[str]:
    """按 [`Urls.NAPCATQQ_DOWNLOAD`](src/core/network/urls.py) +
    [`Urls.MIRROR_SITE`] 顺序构造 "原站优先, 镜像兜底" 的 URL 列表.
    """
    official = Urls.NAPCATQQ_DOWNLOAD.value.toString()
    urls: list[str] = [official]
    for mirror in Urls.MIRROR_SITE.value:
        mirror_str = mirror.toString().rstrip("/")
        urls.append(f"{mirror_str}/{official}")
    return urls


def _download_via_httpx(
    url: str,
    target_path: Path,
    *,
    log_callback: LogLineCallback | None,
    should_cancel: ShouldCancelCallback | None = None,
) -> bool:
    """用 ``httpx.stream`` 把 ``url`` 写到 ``target_path``; 异常视为该源失败.

    原子写: ``target_path.part`` -> rename. 不重试; 由调用方的源列表回退控制重试.

    chunk 循环里每轮检查 ``should_cancel()``, 命中时招 :class:`RemoteDeploymentCancelledError`
    (不被本函数 catch, 直接透出让上层 prefetch 函数同样透出).
    """
    partial = target_path.with_name(target_path.name + ".part")
    try:
        target_path.parent.mkdir(parents=True, exist_ok=True)
        if partial.exists():
            partial.unlink()
        with httpx.stream("GET", url, follow_redirects=True, timeout=30) as resp:
            resp.raise_for_status()
            with partial.open("wb") as out:
                for chunk in resp.iter_bytes():
                    _raise_if_cancelled(should_cancel)
                    out.write(chunk)
        partial.replace(target_path)
        return True
    except RemoteDeploymentCancelledError:
        # 取消: 清理残件后透出, 不要被下面的广谱 except 吞掉
        try:
            if partial.exists():
                partial.unlink()
        except OSError:
            pass
        raise
    except (httpx.RequestError, httpx.HTTPStatusError, OSError) as exc:
        _emit(
            log_callback,
            f"[WARN] 源 {url} 下载失败: {type(exc).__name__}: {exc}",
        )
        try:
            if partial.exists():
                partial.unlink()
        except OSError:
            pass
        return False


def prefetch_napcat_archive_locally(
    *,
    target_path: Path,
    expected_sha512: str | None = None,
    log_callback: LogLineCallback | None = None,
    should_cancel: ShouldCancelCallback | None = None,
) -> Path:
    """把 NapCat.Shell.zip 下载到本机 ``target_path``, 校验 SHA512.

    顺序尝试 [`Urls.NAPCATQQ_DOWNLOAD`](src/core/network/urls.py) +
    [`Urls.MIRROR_SITE`] 直至一源成功. 已存在且 SHA512 一致的本机缓存**直接复用**.

    Args:
        target_path: 本机目标路径, 一般为 ``it(PathFunc).tmp_path / 'NapCat.Shell.zip'``
        expected_sha512: 期望 SHA512 (128 位 hex). 校验失败抛 ``ValueError`` 并删除产物;
            ``None`` 时跳过校验
        log_callback: 部署控制台逐行回调
        should_cancel: 取消检查协议 (参见 :data:`ShouldCancelCallback`); 命中时招
            :class:`RemoteDeploymentCancelledError`. 检查点: 每个源迭代前 + httpx chunk 之间

    Returns:
        ``target_path``, 已确认存在 + (若提供 SHA512) 校验通过.

    Raises:
        RemoteDeploymentCancelledError: 用户中途取消
        RuntimeError: 全部候选源都失败
        ValueError: SHA512 不匹配 (``target_path`` 已被删除)
    """
    target_path = Path(target_path)
    expected_norm = expected_sha512.strip().lower() if expected_sha512 else None

    # 1) 缓存复用
    if target_path.exists():
        if expected_norm is None:
            _emit(log_callback, f"[INFO] 复用本机缓存 (未指定 SHA512): {target_path}")
            return target_path
        actual = _compute_sha512(target_path).lower()
        if actual == expected_norm:
            _emit(log_callback, f"[INFO] 复用本机缓存 (SHA512 一致): {target_path}")
            return target_path
        _emit(
            log_callback,
            f"[WARN] 本机缓存 SHA512 不匹配, 重新下载",
        )
        try:
            target_path.unlink()
        except OSError:
            pass

    # 2) 顺序尝试候选源
    candidates = _build_candidate_urls()
    _emit(log_callback, f"[INFO] 本机预下载 NapCat.Shell.zip, 候选源数={len(candidates)}")
    success = False
    for idx, url in enumerate(candidates, 1):
        # 取消检查点: 每进入下一个源之前 (避免用户点取消后仍要等 7 个 30s 超时走完)
        _raise_if_cancelled(should_cancel)
        _emit(log_callback, f"[INFO] [{idx}/{len(candidates)}] 尝试: {url}")
        if _download_via_httpx(
            url, target_path, log_callback=log_callback, should_cancel=should_cancel
        ):
            _emit(log_callback, f"[INFO] 下载成功 (源 {idx}/{len(candidates)}): {target_path}")
            success = True
            break
    if not success:
        raise RuntimeError(
            f"本机预下载失败: 已尝试 {len(candidates)} 个源都不可用"
        )

    # 3) SHA512 校验
    if expected_norm is not None:
        actual = _compute_sha512(target_path).lower()
        if actual != expected_norm:
            try:
                target_path.unlink()
            except OSError:
                pass
            raise ValueError(
                f"本机预下载产物 SHA512 校验失败: expected={expected_norm} actual={actual}"
            )
        _emit(log_callback, f"[INFO] SHA512 校验通过")

    return target_path


__all__ = (
    "LogLineCallback",
    "ShouldCancelCallback",
    "backend_can_reach_github",
    "prefetch_napcat_archive_locally",
)
