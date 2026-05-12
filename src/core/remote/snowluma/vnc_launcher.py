# -*- coding: utf-8 -*-
"""SnowLuma 远端扫码 noVNC URL 构造 + 浏览器打开 (W10).

OQ4 决策: 首版用 URL query 传 VNC 密码; 文档 caveat 提示用户扫码后关闭 tab.

调用入口::

  daemon = manager.get_or_create_snowluma_daemon(server_id)
  info = daemon.ensure_running()
  url = build_snowluma_novnc_url(info.tunnels.novnc, vnc_password=<read from remote>)
  open_url_in_default_browser(url)

公开 API:

- :func:`build_snowluma_novnc_url`: 构造 noVNC URL (含 password query + autoconnect)
- :func:`read_remote_vnc_password`: SFTP / SSH ``cat`` 读远端 ``vnc.secret``
- :func:`open_snowluma_vnc`: 一键入口 (读密码 + 构造 URL + 调系统浏览器)
"""

from __future__ import annotations

import webbrowser
from urllib.parse import urlencode

from src.core.logging import LogSource, LogType, logger

from ..execution_backend import ExecutionBackend
from .paths import SnowLumaRemotePaths
from .tunnels import SnowLumaTunnelEndpoint


# noVNC 页面相对路径; novnc package 在 Linux 装到 /usr/share/novnc 后, websockify 把
# 它作为静态资源 root, /vnc.html 是默认入口
NOVNC_INDEX_PATH: str = "vnc.html"


def build_snowluma_novnc_url(
    novnc_endpoint: SnowLumaTunnelEndpoint,
    *,
    vnc_password: str,
    autoconnect: bool = True,
    resize: str = "scale",
    view_only: bool = False,
) -> str:
    """构造 noVNC 浏览器 URL.

    Args:
        novnc_endpoint: ``SnowLumaTunnelManager.acquire()`` 返回 bundle 的 novnc 端点
            (含本地端口); URL 形如 ``http://127.0.0.1:47609/vnc.html?...``
        vnc_password: 明文 VNC 密码 (从远端 ``vnc.secret`` 读); noVNC 用这个自动登录
        autoconnect: 页面加载即自动连接 (无需用户点 "Connect")
        resize: ``scale`` / ``downscale`` / ``off`` 之一; noVNC 缩放策略
        view_only: ``True`` 时只看不动 (二维码场景用 False, 让用户可触摸)

    Returns:
        完整 URL 字符串.

    Notes:
        OQ4 caveat: 此 URL 会出现在浏览器历史 / referer; 用户扫码完毕后应主动关闭 tab.
        ephemeral token + URL 短期化方案进 W11 backlog.
    """
    if not vnc_password:
        raise ValueError("vnc_password 必须非空")

    query_params: dict[str, str] = {
        "autoconnect": "1" if autoconnect else "0",
        "resize": resize,
        "password": vnc_password,
        "view_only": "1" if view_only else "0",
        # noVNC 1.4+ 支持 reconnect 自动重连; 远端 daemon 重启时少惊扰用户
        "reconnect": "1",
        "reconnect_delay": "3000",
    }
    return (
        f"{novnc_endpoint.local_url}/{NOVNC_INDEX_PATH}?{urlencode(query_params)}"
    )


def read_remote_vnc_password(
    backend: ExecutionBackend,
    paths: SnowLumaRemotePaths,
) -> str:
    """从远端 ``vnc.secret`` 文件读 VNC 密码 (12 字节 hex).

    Args:
        backend: 已建立 SSH 会话的执行后端
        paths: SL 远端路径

    Returns:
        密码字符串 (去除尾部换行); 空串视为错误.

    Raises:
        RuntimeError: 远端文件不存在 / 读取失败 / 内容为空.
    """
    result = backend.run(f'cat "{paths.vnc_secret}" 2>/dev/null || true', check=False)
    if not result.ok:
        raise RuntimeError(
            f"读取远端 VNC 密码失败 (exit={result.exit_status}): {paths.vnc_secret}"
        )
    password = result.stdout.strip()
    if not password:
        raise RuntimeError(
            f"远端 VNC 密码文件为空或不存在: {paths.vnc_secret} "
            f"(是否还未跑 install_snowluma.sh?)"
        )
    return password


def open_url_in_default_browser(url: str) -> bool:
    """在系统默认浏览器打开 URL.

    Returns:
        ``True`` 表示 webbrowser 模块认为已发起打开请求 (不保证浏览器真打开);
        ``False`` 表示完全失败 (没有可用浏览器).

    Notes:
        Windows 下 :func:`webbrowser.open` 一般调 ``ShellExecute``,
        无需 ``shell=True`` 也不会暴露命令注入面.
    """
    try:
        return webbrowser.open(url, new=2)  # new=2 means new tab
    except Exception as exc:  # noqa: BLE001
        logger.warning(
            f"webbrowser.open 失败: {type(exc).__name__}: {exc}",
            LogType.NETWORK,
            LogSource.CORE,
        )
        return False


def open_snowluma_vnc(
    backend: ExecutionBackend,
    paths: SnowLumaRemotePaths,
    novnc_endpoint: SnowLumaTunnelEndpoint,
    *,
    view_only: bool = False,
) -> tuple[bool, str]:
    """一键打开 SnowLuma 远端扫码界面 (W10 公开入口).

    流程:
        1. SFTP 读远端 ``vnc.secret`` 取密码
        2. 构造 noVNC URL (含 password query + autoconnect)
        3. 调系统浏览器打开

    Args:
        backend: 已建立 SSH 的执行后端 (从 :class:`RemoteSnowLumaDaemon` 间接拿)
        paths: SL 远端路径
        novnc_endpoint: tunnel manager bundle 的 novnc 端点
        view_only: ``True`` 时只看不动 (通常 False, 用户需触摸扫码)

    Returns:
        ``(成功标志, 用户可读消息)``;

        - 成功时第二项是用户可读的 noVNC 端点描述 (``http://127.0.0.1:<port>``);
          **不含明文密码**, 防止调用方误把它写到日志/UI 弹窗 (P10 review).
        - 失败时第二项是错误描述.

    Notes:
        构造好的 noVNC URL (含明文 ``password=`` query) 仅经 :func:`webbrowser.open`
        交给系统浏览器, **不返回给 Python 调用方**. 此设计配合 OQ4 决策的 "URL 仅短期
        存在于浏览器历史" 安全模型: Python 进程内不留密码副本.
    """
    try:
        password = read_remote_vnc_password(backend, paths)
    except RuntimeError as exc:
        return False, str(exc)

    url = build_snowluma_novnc_url(
        novnc_endpoint,
        vnc_password=password,
        view_only=view_only,
    )

    if not open_url_in_default_browser(url):
        return False, "未能调起系统浏览器, 请手动复制 URL 打开"

    logger.info(
        f"SnowLuma noVNC 已打开 (local_port={novnc_endpoint.local_port})",
        LogType.NETWORK,
        LogSource.CORE,
    )
    # P10 (review): 返回脱敏端点描述; URL 含明文密码, 不进 Python 调用栈
    return True, novnc_endpoint.local_url


__all__ = [
    "build_snowluma_novnc_url",
    "read_remote_vnc_password",
    "open_url_in_default_browser",
    "open_snowluma_vnc",
    "NOVNC_INDEX_PATH",
]
