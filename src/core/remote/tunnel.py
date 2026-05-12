# -*- coding: utf-8 -*-
"""SSH 本地端口转发 (P2.5).

为 [`RemoteBackend.get_webui_endpoint`](src/core/operation/remote_backend.py)
提供"远端 NapCat WebUI 端口 -> 本地 loopback 端口"的隧道, 让 Desktop 现有
[`GetAuthStatusRunnable`](src/core/runtime/napcat.py) /
[`GetLoginStatusRunnable`](src/core/runtime/napcat.py) 等访问 ``http://127.0.0.1:<port>``
的代码无需改动即可对接远端 NapCat.

实现原理:
- 在 Desktop 进程内绑定 ``127.0.0.1:0`` 起一个 [`socketserver.ThreadingTCPServer`](https://docs.python.org/3/library/socketserver.html)
- 每个本地连接落入 [`_ForwardingHandler`](src/core/remote/tunnel.py) 后, 通过
  [`paramiko.Transport.open_channel('direct-tcpip', ...)`](https://docs.paramiko.org/en/stable/api/transport.html#paramiko.transport.Transport.open_channel)
  在已建立的 SSH 会话上开通一条到远端 ``127.0.0.1:<remote_port>`` 的子通道,
  然后做双向字节泵.
- 隧道关闭时关闭所有 channel 与 server, 释放本地端口.

设计要点:
- 隧道仅监听 loopback (``127.0.0.1``), 永远不暴露到外网.
- 隧道复用同一个 SSH ``Transport``, 不会触发额外认证.
- 单个隧道对应单个 ``(remote_host, remote_port)`` 组合; 同一 Bot 重启后远端端口可能漂移,
  上层应当在端口变化时关闭旧隧道再新开一条.

参考:
- paramiko 官方示例 [forward.py](https://github.com/paramiko/paramiko/blob/main/demos/forward.py).
- [`docs/general/remote_ssh_plan.md`](../../../../docs/general/remote_ssh_plan.md) §5.1 (方案 A).
"""

from __future__ import annotations

import select
import socket
import socketserver
import threading
from typing import TYPE_CHECKING

from src.core.logging import LogSource, LogType, logger

from .errors import SSHConnectionError

if TYPE_CHECKING:
    import paramiko


# 单次 select / channel 读写循环的最大块大小; 1024 与 paramiko forward.py 示例对齐.
_TUNNEL_CHUNK_SIZE = 1024


class _ForwardingHandler(socketserver.BaseRequestHandler):
    """每条本地 TCP 连接对应的转发处理器.

    在 ``handle()`` 中通过 SSH ``Transport`` 打开一条 ``direct-tcpip`` 子通道,
    然后用 ``select`` 在 ``self.request`` (本地 socket) 与 ``channel`` 之间互泵.
    """

    # 这两个属性由 [`LocalPortForwarder._build_handler_class`](src/core/remote/tunnel.py)
    # 注入到子类上, 而非通过 ``__init__``, 因为 socketserver 框架会自己实例化 handler.
    transport: "paramiko.Transport"
    remote_host: str
    remote_port: int
    label: str

    def handle(self) -> None:  # noqa: D401 - socketserver 框架约定的方法名
        """转发单条本地 TCP 连接到远端 SSH ``direct-tcpip`` 通道."""
        peer = self.request.getpeername()
        try:
            channel = self.transport.open_channel(
                "direct-tcpip",
                (self.remote_host, self.remote_port),
                peer,
            )
        except Exception as exc:  # noqa: BLE001 - paramiko 会抛多种类型, 统一捕获
            logger.warning(
                f"SSH 隧道 {self.label} 打开 direct-tcpip channel 失败: {exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            try:
                self.request.close()
            except OSError:
                pass
            return

        if channel is None:
            logger.warning(
                f"SSH 隧道 {self.label} 拒绝建立 channel (远端 sshd 可能禁用了 AllowTcpForwarding)",
                LogType.NETWORK,
                LogSource.CORE,
            )
            try:
                self.request.close()
            except OSError:
                pass
            return

        try:
            self._pump(channel)
        finally:
            try:
                channel.close()
            except Exception:  # noqa: BLE001
                pass
            try:
                self.request.close()
            except OSError:
                pass

    def _pump(self, channel: "paramiko.Channel") -> None:
        """双向字节泵; 任一方向关闭即退出."""
        while True:
            try:
                readable, _, _ = select.select([self.request, channel], [], [])
            except (OSError, ValueError):
                # 套接字已关闭, 直接退出
                return

            if self.request in readable:
                try:
                    data = self.request.recv(_TUNNEL_CHUNK_SIZE)
                except OSError:
                    return
                if not data:
                    return
                channel.sendall(data)

            if channel in readable:
                try:
                    data = channel.recv(_TUNNEL_CHUNK_SIZE)
                except Exception:  # noqa: BLE001
                    return
                if not data:
                    return
                try:
                    self.request.sendall(data)
                except OSError:
                    return


class _ThreadingTCPServer(socketserver.ThreadingTCPServer):
    """允许快速重新绑定监听端口."""

    daemon_threads = True
    allow_reuse_address = True


class LocalPortForwarder:
    """在已有 SSH 会话上提供 ``127.0.0.1:<local_port> -> remote_host:remote_port`` 端口转发.

    生命周期:
    1. ``forwarder = LocalPortForwarder(transport, remote_host, remote_port)``
    2. ``local_port = forwarder.start()`` (绑定 loopback + 启动后台线程)
    3. Desktop 通过 ``http://127.0.0.1:<local_port>`` 访问远端 NapCat WebUI
    4. ``forwarder.stop()`` 关闭监听 + 释放端口

    所有方法**线程安全**, ``start`` / ``stop`` 各自做幂等.
    """

    def __init__(
        self,
        transport: "paramiko.Transport",
        remote_host: str,
        remote_port: int,
        *,
        label: str = "tunnel",
        preferred_local_port: int = 0,
    ) -> None:
        """
        Args:
            preferred_local_port: 优先绑定的本地端口; ``0`` 让 OS 选随机端口 (NC 历史默认).
                W6 SnowLumaTunnelManager 用 47099/47609 实现 "noVNC URL 端口稳定" 体验,
                绑定失败时 (端口已被占用) 抛 :class:`SSHConnectionError`, 调用方可捕获
                后传 ``0`` 回退随机.
        """
        self._transport = transport
        self._remote_host = remote_host
        self._remote_port = remote_port
        self._label = label
        self._preferred_local_port = preferred_local_port
        self._server: _ThreadingTCPServer | None = None
        self._thread: threading.Thread | None = None
        self._lock = threading.Lock()

    @property
    def is_running(self) -> bool:
        return self._server is not None

    @property
    def local_port(self) -> int | None:
        if self._server is None:
            return None
        return self._server.server_address[1]  # type: ignore[index]

    @property
    def remote_host(self) -> str:
        return self._remote_host

    @property
    def remote_port(self) -> int:
        return self._remote_port

    def start(self) -> int:
        """绑定本地随机端口并开始转发, 返回绑定的本地端口."""
        with self._lock:
            if self._server is not None:
                port = self._server.server_address[1]  # type: ignore[index]
                return port

            handler_class = self._build_handler_class()
            try:
                server = _ThreadingTCPServer(
                    ("127.0.0.1", self._preferred_local_port), handler_class
                )
            except OSError as exc:
                raise SSHConnectionError(
                    f"SSH 隧道 {self._label} 无法绑定本地端口 "
                    f"(preferred={self._preferred_local_port}): {exc}"
                ) from exc

            thread = threading.Thread(
                target=server.serve_forever,
                name=f"ssh-tunnel-{self._label}",
                daemon=True,
            )
            thread.start()

            self._server = server
            self._thread = thread

            local_port = server.server_address[1]  # type: ignore[index]
            logger.info(
                (
                    "SSH 隧道已建立: "
                    f"label={self._label}, local=127.0.0.1:{local_port}, "
                    f"remote={self._remote_host}:{self._remote_port}"
                ),
                LogType.NETWORK,
                LogSource.CORE,
            )
            return local_port

    def stop(self) -> None:
        """关闭监听端口与所有活跃 channel; 幂等."""
        with self._lock:
            server = self._server
            thread = self._thread
            self._server = None
            self._thread = None

        if server is None:
            return

        try:
            server.shutdown()
        except Exception:  # noqa: BLE001
            pass
        try:
            server.server_close()
        except Exception:  # noqa: BLE001
            pass
        if thread is not None and thread.is_alive():
            thread.join(timeout=5.0)

        logger.info(
            f"SSH 隧道已关闭: label={self._label}",
            LogType.NETWORK,
            LogSource.CORE,
        )

    def __enter__(self) -> "LocalPortForwarder":
        self.start()
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        self.stop()

    def _build_handler_class(self) -> type[_ForwardingHandler]:
        """动态生成绑定了当前 transport / remote 的 handler 子类.

        socketserver 会自行实例化 handler, 没有原生方式向其传参,
        所以把上下文当作类属性挂在子类上.
        """
        transport = self._transport
        remote_host = self._remote_host
        remote_port = self._remote_port
        label = self._label

        class _BoundHandler(_ForwardingHandler):
            pass

        _BoundHandler.transport = transport
        _BoundHandler.remote_host = remote_host
        _BoundHandler.remote_port = remote_port
        _BoundHandler.label = label
        return _BoundHandler
