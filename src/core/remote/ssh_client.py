# -*- coding: utf-8 -*-
"""SSH 客户端封装。"""

from __future__ import annotations

import shlex
import socket
import threading
from collections.abc import Callable
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING, Any, TypeVar

from src.core.logging import LogSource, LogType, logger

from .errors import RemoteCommandError, SSHAuthenticationError, SSHConnectionError, SSHHostKeyError
from .models import RemoteCommandResult, SSHCredentials

if TYPE_CHECKING:
    from .tunnel import LocalPortForwarder

try:
    import paramiko
except ImportError:  # pragma: no cover - 依赖缺失时在运行期给出清晰错误
    paramiko = None

_T = TypeVar("_T")


class _LineSplitter:
    """累积式行切分器, 用于 [`SSHClient.exec_stream`](src/core/remote/ssh_client.py)。

    切分规则:

    - ``\\r\\n`` 视为一个换行 (避免 PTY 模式下行尾 CRLF 产生空白行)
    - 单独的 ``\\r`` 也作为换行 (curl 进度条等场景)
    - 单独的 ``\\n`` 作为换行
    - ``\\r`` 落在缓冲最末尾时延迟到下一次 ``feed`` 再决断, 防止跨读取边界的 CRLF 退化为两次切分

    线程不安全, 每个 SSH 通道独占一个实例。
    """

    __slots__ = ("_buffer",)

    def __init__(self) -> None:
        self._buffer = ""

    def feed(self, chunk: str) -> list[str]:
        """喂入新读到的字符串, 返回本次新切出的行(不含行尾)。"""
        self._buffer += chunk
        lines: list[str] = []
        while True:
            cr_pos = self._buffer.find("\r")
            lf_pos = self._buffer.find("\n")
            candidates = [pos for pos in (cr_pos, lf_pos) if pos != -1]
            if not candidates:
                break
            cut = min(candidates)
            # 边界: \r 在末尾时延迟, 等下一次 feed 来确认是 CRLF 还是孤立 \r
            if self._buffer[cut] == "\r" and cut == len(self._buffer) - 1:
                break
            line = self._buffer[:cut]
            # \r\n 紧挨着, 一次性消费两个字符
            if (
                self._buffer[cut] == "\r"
                and cut + 1 < len(self._buffer)
                and self._buffer[cut + 1] == "\n"
            ):
                self._buffer = self._buffer[cut + 2 :]
            else:
                self._buffer = self._buffer[cut + 1 :]
            lines.append(line)
        return lines

    def flush(self) -> list[str]:
        """流读取结束时调用, 把残留缓冲作为最后一行返回。"""
        if not self._buffer:
            return []
        last = self._buffer
        self._buffer = ""
        return [last]


class SSHClient:
    """受限默认配置的 SSH 客户端。

    默认策略：
    - 拒绝未知主机指纹
    - 关闭 agent 与自动搜索本地密钥
    - 默认不使用 PTY，避免引入额外 shell 行为差异

    P3.W1: 持久连接 + 自动重连 (参考 [`docs/general/remote_ssh_p3_plan.md`](../../../../docs/general/remote_ssh_p3_plan.md) §3.1):
    - ``connect()`` 后自动启用 ``transport.set_keepalive(DEFAULT_KEEPALIVE_INTERVAL)``
      让 paramiko 周期性发心跳, 防止 NAT / 中间设备静默切断 idle 连接
    - 所有 SSH/SFTP 入口走 [`_call_with_retry`](src/core/remote/ssh_client.py) 包装,
      检测到 transport 在调用过程中死亡后自动重连一次, 并重试该次操作
    - 重连过程使用 [`_reconnect_lock`](src/core/remote/ssh_client.py) 互斥, 多个 worker
      并发触发时只会有一次实际 SSH 握手
    """

    #: 默认 keepalive 间隔 (秒). paramiko 会按此周期发空闲消息保活,
    #: 同时也是 transport 检测对端断开的最坏粒度.
    DEFAULT_KEEPALIVE_INTERVAL: int = 30

    def __init__(self, credentials: SSHCredentials) -> None:
        self.credentials = credentials
        self._client: "paramiko.SSHClient | None" = None
        self._remote_home_dir: str | None = None
        # P3.W1: 序列化 ensure_alive / connect / close, 防止多个 QThreadPool worker
        # 在断线瞬间同时触发重连导致 race.
        self._reconnect_lock = threading.RLock()

    def connect(self) -> None:
        """建立 SSH 连接。"""
        self.credentials.validate()
        self._ensure_paramiko_available()

        client = paramiko.SSHClient()
        self._apply_host_key_policy(client)

        connect_kwargs: dict[str, Any] = {
            "hostname": self.credentials.host,
            "port": self.credentials.port,
            "username": self.credentials.username,
            "timeout": self.credentials.connect_timeout,
            "auth_timeout": self.credentials.connect_timeout,
            "banner_timeout": self.credentials.connect_timeout,
            "allow_agent": self.credentials.allow_agent,
            "look_for_keys": self.credentials.look_for_keys,
        }
        if self.credentials.auth_method == "password":
            connect_kwargs["password"] = self.credentials.password
        else:
            connect_kwargs["key_filename"] = str(self.credentials.private_key_file)
            if self.credentials.private_key_passphrase:
                connect_kwargs["passphrase"] = self.credentials.private_key_passphrase

        logger.info(
            (
                "准备建立 SSH 连接: "
                f"host={self.credentials.host}, port={self.credentials.port}, "
                f"username={self.credentials.username}, auth_method={self.credentials.auth_method}, "
                f"host_key_policy={self.credentials.host_key_policy}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )

        try:
            client.connect(**connect_kwargs)
        except paramiko.BadHostKeyException as exc:
            raise SSHHostKeyError(f"SSH 主机指纹校验失败: {exc}") from exc
        except paramiko.AuthenticationException as exc:
            raise SSHAuthenticationError(f"SSH 认证失败: {exc}") from exc
        except (paramiko.SSHException, socket.timeout, TimeoutError, OSError) as exc:
            raise SSHConnectionError(f"SSH 连接失败: {exc}") from exc

        self._client = client
        # P3.W1: 启用 keepalive, 让 paramiko 周期性发空闲消息保活;
        # 服务端默认 ``ClientAliveInterval=0`` 时也能撑住 NAT idle timeout.
        # 失败被 paramiko 静默吞, 不会影响主连接, 此处只在 trace 级别记录.
        try:
            transport = client.get_transport()
            if transport is not None:
                transport.set_keepalive(self.DEFAULT_KEEPALIVE_INTERVAL)
        except Exception as exc:  # noqa: BLE001 - 任何异常都不应阻断已建立的连接
            logger.trace(
                f"SSH set_keepalive 调用失败 (忽略): host={self.credentials.host}, exc={exc!r}",
                LogType.NETWORK,
                LogSource.CORE,
            )
        logger.info(
            f"SSH 连接已建立: host={self.credentials.host}, username={self.credentials.username}",
            LogType.NETWORK,
            LogSource.CORE,
        )

    def close(self) -> None:
        """关闭 SSH 连接。"""
        with self._reconnect_lock:
            if self._client is None:
                return

            self._client.close()
            self._client = None
            self._remote_home_dir = None
            logger.info(
                f"SSH 连接已关闭: host={self.credentials.host}, username={self.credentials.username}",
                LogType.NETWORK,
                LogSource.CORE,
            )

    def ensure_alive(self, *, reconnect: bool = True) -> bool:
        """探测当前 SSH 会话是否仍可用; 必要时自动重连 **一次**.

        P3.W1 自愈语义:
        - ``is_connected`` 为真直接返回 True
        - ``reconnect=False`` 时, 仅做探测不发起新连接, 返回当前状态
        - ``reconnect=True`` 时, 丢弃死会话并调用一次 [`connect`](src/core/remote/ssh_client.py),
          失败时返回 False, 调用方决定是抛错还是降级

        Args:
            reconnect: 是否在会话死亡时自动重连; 默认 True

        Returns:
            布尔值, 表示返回时刻 SSH 会话是否可用
        """
        with self._reconnect_lock:
            if self.is_connected:
                return True
            if not reconnect:
                return False

            host = self.credentials.host
            logger.warning(
                f"SSH 会话已断开, 尝试自动重连一次: host={host}, username={self.credentials.username}",
                LogType.NETWORK,
                LogSource.CORE,
            )

            # 释放半死的旧 client (paramiko 会在 close 时尽力清理 transport)
            if self._client is not None:
                try:
                    self._client.close()
                except Exception:  # noqa: BLE001 - 死会话 close 失败不影响重连
                    pass
            self._client = None
            self._remote_home_dir = None

            try:
                self.connect()
            except (SSHConnectionError, SSHAuthenticationError, SSHHostKeyError) as exc:
                logger.warning(
                    f"SSH 自动重连失败: host={host}, exc={exc!r}",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
                return False

            logger.info(
                f"SSH 自动重连成功: host={host}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            return self.is_connected

    def _call_with_retry(self, op: Callable[[], _T], *, label: str) -> _T:
        """在 SSH/SFTP 操作外层做"前置探活 + 失败后重连一次"包装.

        语义:
        1. 调用前若 ``is_connected==False`` 则尝试自动重连 (一次)
        2. 调用 ``op``; 抛 [`SSHConnectionError`](src/core/remote/errors.py) 时:
           - 若此刻 ``is_connected`` 仍为真 (例如命令超时, transport 还活着) -> 原样抛出
           - 若 transport 死亡 -> 重连一次后重试 ``op`` **一次**, 仍失败则抛出
        3. 其他异常 (``RemoteCommandError`` 等) 一律不重试

        Args:
            op: 包装好的操作闭包, 应是幂等或安全可重试的 SSH/SFTP 调用
            label: 仅用于日志的简短标签

        Returns:
            ``op`` 的返回值
        """
        # 前置探活: 调用方可能是断线很久后第一次发包, 此处便宜地恢复一次
        if not self.is_connected:
            self.ensure_alive(reconnect=True)

        try:
            return op()
        except SSHConnectionError as exc:
            # transport 活着但命令超时 / 远端卡住, 不应重试
            if self.is_connected:
                raise
            logger.warning(
                f"SSH 操作 {label} 失败且会话已断开: {exc!r}; 准备自动重连后重试一次",
                LogType.NETWORK,
                LogSource.CORE,
            )
            if not self.ensure_alive(reconnect=True):
                raise
            try:
                return op()
            except SSHConnectionError as retry_exc:
                logger.warning(
                    f"SSH 操作 {label} 自动重连后仍失败: {retry_exc!r}",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
                raise

    def run(self, command: str, *, timeout: float | None = None, get_pty: bool = False, check: bool = False) -> RemoteCommandResult:
        """执行远程命令。"""
        return self._call_with_retry(
            lambda: self._run_once(command, timeout=timeout, get_pty=get_pty, check=check),
            label="run",
        )

    def _run_once(
        self,
        command: str,
        *,
        timeout: float | None,
        get_pty: bool,
        check: bool,
    ) -> RemoteCommandResult:
        """``run`` 的实际执行体, 不含 [`_call_with_retry`](src/core/remote/ssh_client.py) 包装."""
        client = self._require_client()
        effective_timeout = timeout or self.credentials.command_timeout

        logger.trace(
            (
                "执行远程命令: "
                f"host={self.credentials.host}, timeout={effective_timeout}, get_pty={get_pty}, command={command}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )

        try:
            _stdin, stdout, stderr = client.exec_command(command, timeout=effective_timeout, get_pty=get_pty)
            exit_status = stdout.channel.recv_exit_status()
            result = RemoteCommandResult(
                command=command,
                exit_status=exit_status,
                stdout=stdout.read().decode("utf-8", errors="replace"),
                stderr=stderr.read().decode("utf-8", errors="replace"),
            )
        except (socket.timeout, TimeoutError) as exc:
            raise SSHConnectionError(
                f"远程命令在 {effective_timeout:.0f}s 内无新输出而被 SSH 层超时中断 "
                f"(可能是远端阻塞或网络问题): {exc!r}"
            ) from exc
        except (paramiko.SSHException, OSError) as exc:
            raise SSHConnectionError(f"远程命令执行异常: {exc!r}") from exc

        if check and not result.ok:
            raise RemoteCommandError(command=result.command, exit_status=result.exit_status, stderr=result.stderr)
        return result

    def exec_stream(
        self,
        command: str,
        *,
        on_stdout_line: Callable[[str], None] | None = None,
        on_stderr_line: Callable[[str], None] | None = None,
        timeout: float | None = None,
        check: bool = False,
        merge_stderr: bool = False,
    ) -> RemoteCommandResult:
        # P3.W1: 流式命令重试需谨慎 — 一旦开始有输出回流给上层 (例如部署脚本进度),
        # 整段重跑可能触发"双倍下载 / 二次写盘"等副作用. 因此只做"前置探活"(如果
        # 入口前就发现连接已死, 自愈一次), 命令开始执行后任何错误都原样上抛,
        # 由调用方决定是否重试.
        if not self.is_connected:
            self.ensure_alive(reconnect=True)
        return self._exec_stream_once(
            command,
            on_stdout_line=on_stdout_line,
            on_stderr_line=on_stderr_line,
            timeout=timeout,
            check=check,
            merge_stderr=merge_stderr,
        )

    def _exec_stream_once(
        self,
        command: str,
        *,
        on_stdout_line: Callable[[str], None] | None = None,
        on_stderr_line: Callable[[str], None] | None = None,
        timeout: float | None = None,
        check: bool = False,
        merge_stderr: bool = False,
    ) -> RemoteCommandResult:
        """执行远程命令并流式读取 stdout / stderr。

        与 [`run`](src/core/remote/ssh_client.py) 不同, 该方法在命令仍在运行时即可
        通过回调把每一行 stdout / stderr 投递给上层, 用于解析 P1 部署脚本的
        ``[PROGRESS] N message`` 进度协议。

        Args:
            command: 远端 shell 命令
            on_stdout_line: 每收到一行 stdout 触发的回调; 异常会被捕获并记录但不会中断命令
            on_stderr_line: 每收到一行 stderr 触发的回调
            timeout: 单次命令的最大耗时(秒); 默认使用凭据的 ``command_timeout``
            check: 退出码非 0 时抛 [`RemoteCommandError`](src/core/remote/errors.py)
            merge_stderr: 当为 True 时启用 PTY, 远端 bash 进入行缓冲, 且 stderr 会合并到 stdout
                并实时发往 ``on_stdout_line``。适合"展示部署终端"场景, 牺牲 stream 区分度换取实时性。

        Returns:
            完整的命令结果, ``stdout`` / ``stderr`` 为流式累积后的合并文本(以 ``\n`` 分隔)。
            当 ``merge_stderr=True`` 时, ``stderr`` 字段为空, 所有输出都进入 ``stdout``。
        """
        client = self._require_client()
        effective_timeout = timeout or self.credentials.command_timeout

        logger.trace(
            (
                "执行流式远程命令: "
                f"host={self.credentials.host}, timeout={effective_timeout}, "
                f"merge_stderr={merge_stderr}, command={command}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )

        try:
            _stdin, stdout, stderr = client.exec_command(
                command,
                timeout=effective_timeout,
                get_pty=merge_stderr,
            )
            captured_stdout: list[str] = []
            captured_stderr: list[str] = []

            # PTY 模式下行尾可能是 ``\r\n``; 同时 ``readline`` 在分块读取时
            # 部分行可能只到 ``\r``。统一按 ``\r`` 与 ``\n`` 切分以避免 curl 等
            # 工具的 carriage-return 进度条吃掉多行内容。
            splitter = _LineSplitter()
            for raw_line in iter(stdout.readline, ""):
                for line in splitter.feed(raw_line):
                    captured_stdout.append(line)
                    if on_stdout_line is not None:
                        try:
                            on_stdout_line(line)
                        except Exception as exc:  # noqa: BLE001 - 回调失败不应中断命令
                            logger.warning(
                                f"on_stdout_line 回调异常: {exc}",
                                LogType.NETWORK,
                                LogSource.CORE,
                            )
            # 缓冲尾部的最后一行 (没有终结换行) 也要发出
            for line in splitter.flush():
                captured_stdout.append(line)
                if on_stdout_line is not None:
                    try:
                        on_stdout_line(line)
                    except Exception as exc:  # noqa: BLE001
                        logger.warning(
                            f"on_stdout_line 回调异常: {exc}",
                            LogType.NETWORK,
                            LogSource.CORE,
                        )

            exit_status = stdout.channel.recv_exit_status()

            # PTY 模式下 stderr 已经混入 stdout, paramiko 的 stderr 流通常为空
            if not merge_stderr:
                stderr_text = stderr.read().decode("utf-8", errors="replace")
                if stderr_text:
                    for line in stderr_text.splitlines():
                        captured_stderr.append(line)
                        if on_stderr_line is not None:
                            try:
                                on_stderr_line(line)
                            except Exception as exc:  # noqa: BLE001
                                logger.warning(
                                    f"on_stderr_line 回调异常: {exc}",
                                    LogType.NETWORK,
                                    LogSource.CORE,
                                )

            result = RemoteCommandResult(
                command=command,
                exit_status=exit_status,
                stdout="\n".join(captured_stdout),
                stderr="\n".join(captured_stderr),
            )
        except (socket.timeout, TimeoutError) as exc:
            raise SSHConnectionError(
                f"远程命令在 {effective_timeout:.0f}s 内无新输出而被 SSH 层超时中断 "
                f"(可能是远端阻塞或网络问题): {exc!r}"
            ) from exc
        except (paramiko.SSHException, OSError) as exc:
            raise SSHConnectionError(f"远程命令执行异常: {exc!r}") from exc

        if check and not result.ok:
            raise RemoteCommandError(command=result.command, exit_status=result.exit_status, stderr=result.stderr)
        return result

    def ensure_remote_directory(self, remote_path: str) -> RemoteCommandResult:
        """确保远端目录存在。"""
        return self.run(f"mkdir -p -- {self._quote_remote_argument(remote_path)}", check=True)

    def upload_file(self, local_path: str | Path, remote_path: str) -> None:
        """上传单个文件。"""
        return self._call_with_retry(
            lambda: self._upload_file_once(local_path, remote_path),
            label="upload_file",
        )

    def _upload_file_once(self, local_path: str | Path, remote_path: str) -> None:
        client = self._require_client()
        local_file = Path(local_path)
        if not local_file.exists():
            raise FileNotFoundError(f"待上传文件不存在: {local_file}")
        resolved_remote_path = self._resolve_sftp_path(remote_path)

        logger.info(
            (
                "开始上传远端文件: "
                f"local={local_file}, remote={remote_path}, resolved_remote={resolved_remote_path}, "
                f"size={local_file.stat().st_size}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )
        self.ensure_remote_directory(PurePosixPath(remote_path).parent.as_posix())

        try:
            with client.open_sftp() as sftp:
                sftp.put(str(local_file), resolved_remote_path)
        except (OSError, paramiko.SSHException) as exc:
            raise SSHConnectionError(f"上传文件失败: {exc}") from exc
        logger.info(
            f"远端文件上传完成: remote={resolved_remote_path}",
            LogType.NETWORK,
            LogSource.CORE,
        )

    def download_file(self, remote_path: str, local_path: str | Path) -> None:
        """下载单个文件。"""
        return self._call_with_retry(
            lambda: self._download_file_once(remote_path, local_path),
            label="download_file",
        )

    def _download_file_once(self, remote_path: str, local_path: str | Path) -> None:
        client = self._require_client()
        local_file = Path(local_path)
        local_file.parent.mkdir(parents=True, exist_ok=True)
        resolved_remote_path = self._resolve_sftp_path(remote_path)

        try:
            with client.open_sftp() as sftp:
                sftp.get(resolved_remote_path, str(local_file))
        except (OSError, paramiko.SSHException) as exc:
            raise SSHConnectionError(f"下载文件失败: {exc}") from exc

    def read_text(self, remote_path: str, *, encoding: str = "utf-8") -> str:
        """读取远端文本文件全部内容。"""
        return self._call_with_retry(
            lambda: self._read_text_once(remote_path, encoding=encoding),
            label="read_text",
        )

    def _read_text_once(self, remote_path: str, *, encoding: str) -> str:
        client = self._require_client()
        resolved = self._resolve_sftp_path(remote_path)
        try:
            with client.open_sftp() as sftp, sftp.open(resolved, "rb") as handle:
                data = handle.read()
        except (OSError, paramiko.SSHException) as exc:
            raise SSHConnectionError(f"读取远端文件失败: {exc}") from exc
        return data.decode(encoding, errors="replace")

    def write_text(self, remote_path: str, content: str, *, encoding: str = "utf-8") -> None:
        """写入远端文本文件, 父目录不存在时自动创建。"""
        return self._call_with_retry(
            lambda: self._write_text_once(remote_path, content, encoding=encoding),
            label="write_text",
        )

    def _write_text_once(self, remote_path: str, content: str, *, encoding: str) -> None:
        client = self._require_client()
        resolved = self._resolve_sftp_path(remote_path)
        parent = PurePosixPath(remote_path).parent.as_posix()
        if parent and parent != ".":
            self.ensure_remote_directory(parent)
        try:
            with client.open_sftp() as sftp, sftp.open(resolved, "wb") as handle:
                handle.write(content.encode(encoding))
        except (OSError, paramiko.SSHException) as exc:
            raise SSHConnectionError(f"写入远端文件失败: {exc}") from exc

    def remote_exists(self, remote_path: str) -> bool:
        """判断远端路径是否存在(文件或目录)。"""
        return self._call_with_retry(
            lambda: self._remote_exists_once(remote_path),
            label="remote_exists",
        )

    def _remote_exists_once(self, remote_path: str) -> bool:
        client = self._require_client()
        resolved = self._resolve_sftp_path(remote_path)
        try:
            with client.open_sftp() as sftp:
                try:
                    sftp.stat(resolved)
                    return True
                except FileNotFoundError:
                    return False
        except (OSError, paramiko.SSHException) as exc:
            raise SSHConnectionError(f"探测远端路径失败: {exc}") from exc

    def remote_listdir(self, remote_path: str) -> list[tuple[str, bool, int]]:
        """列出远端目录条目, 返回 ``(name, is_dir, size)`` 三元组列表。"""
        return self._call_with_retry(
            lambda: self._remote_listdir_once(remote_path),
            label="remote_listdir",
        )

    def _remote_listdir_once(self, remote_path: str) -> list[tuple[str, bool, int]]:
        import stat as _stat

        client = self._require_client()
        resolved = self._resolve_sftp_path(remote_path)
        try:
            with client.open_sftp() as sftp:
                entries = sftp.listdir_attr(resolved)
        except (OSError, paramiko.SSHException) as exc:
            raise SSHConnectionError(f"列出远端目录失败: {exc}") from exc
        return [
            (entry.filename, _stat.S_ISDIR(entry.st_mode or 0), entry.st_size or 0)
            for entry in entries
        ]

    def remote_remove(self, remote_path: str, *, recursive: bool = False) -> None:
        """删除远端文件或目录(目录需 ``recursive=True``)。"""
        # 内部已经走 ``run``, 自带重试; 此处不再额外包一层避免重复重试.
        quoted = self._quote_remote_argument(remote_path)
        if recursive:
            self.run(f"rm -rf -- {quoted}", check=True)
        else:
            self.run(f"rm -f -- {quoted}", check=True)

    # ==================== 字节级 IO (P4 W3 F6 持久数据迁移) ====================
    def remote_file_size(self, remote_path: str) -> int:
        """``sftp.stat(path).st_size``; 远端不存在时抛 ``FileNotFoundError``."""
        return self._call_with_retry(
            lambda: self._remote_file_size_once(remote_path),
            label="remote_file_size",
        )

    def _remote_file_size_once(self, remote_path: str) -> int:
        client = self._require_client()
        resolved = self._resolve_sftp_path(remote_path)
        try:
            with client.open_sftp() as sftp:
                attr = sftp.stat(resolved)
        except FileNotFoundError:
            raise
        except (OSError, paramiko.SSHException) as exc:
            raise SSHConnectionError(f"获取远端文件大小失败: {exc}") from exc
        size = getattr(attr, "st_size", None)
        return int(size) if size is not None else 0

    def read_bytes(
        self,
        remote_path: str,
        *,
        offset: int = 0,
        length: int | None = None,
    ) -> bytes:
        """从远端文件 ``remote_path`` 的 ``offset`` 起读 ``length`` 字节; ``length=None`` 读到结尾."""
        if offset < 0:
            raise ValueError(f"offset 不能为负数: {offset}")
        return self._call_with_retry(
            lambda: self._read_bytes_once(remote_path, offset=offset, length=length),
            label="read_bytes",
        )

    def _read_bytes_once(
        self,
        remote_path: str,
        *,
        offset: int,
        length: int | None,
    ) -> bytes:
        client = self._require_client()
        resolved = self._resolve_sftp_path(remote_path)
        try:
            with client.open_sftp() as sftp, sftp.open(resolved, "rb") as handle:
                if offset:
                    handle.seek(offset)
                if length is None:
                    return bytes(handle.read())
                return bytes(handle.read(length))
        except FileNotFoundError:
            raise
        except (OSError, paramiko.SSHException) as exc:
            raise SSHConnectionError(f"读取远端文件字节失败: {exc}") from exc

    def append_bytes(self, remote_path: str, data: bytes) -> None:
        """以 append 模式向 ``remote_path`` 追加 ``data``; 父目录不存在时自动创建."""
        return self._call_with_retry(
            lambda: self._append_bytes_once(remote_path, data),
            label="append_bytes",
        )

    def _append_bytes_once(self, remote_path: str, data: bytes) -> None:
        client = self._require_client()
        resolved = self._resolve_sftp_path(remote_path)
        parent = PurePosixPath(remote_path).parent.as_posix()
        if parent and parent != ".":
            self.ensure_remote_directory(parent)
        try:
            with client.open_sftp() as sftp, sftp.open(resolved, "ab") as handle:
                handle.write(data)
        except (OSError, paramiko.SSHException) as exc:
            raise SSHConnectionError(f"追加远端文件失败: {exc}") from exc

    def remote_rename(self, src: str, dst: str) -> None:
        """通过 ``sftp.posix_rename`` (若可用) 原子重命名 / 覆盖; 退化为 ``sftp.rename``.

        SFTP 标准的 ``rename`` 在 dst 已存在时行为依赖服务端实现; 大多数 OpenSSH
        后端实现 ``posix-rename@openssh.com`` 扩展, 提供 ``posix_rename`` 覆盖语义.
        """
        return self._call_with_retry(
            lambda: self._remote_rename_once(src, dst),
            label="remote_rename",
        )

    def _remote_rename_once(self, src: str, dst: str) -> None:
        client = self._require_client()
        src_resolved = self._resolve_sftp_path(src)
        dst_resolved = self._resolve_sftp_path(dst)
        parent = PurePosixPath(dst).parent.as_posix()
        if parent and parent != ".":
            self.ensure_remote_directory(parent)
        try:
            with client.open_sftp() as sftp:
                # 优先 posix_rename: dst 已存在时原子覆盖
                posix_rename = getattr(sftp, "posix_rename", None)
                if callable(posix_rename):
                    posix_rename(src_resolved, dst_resolved)
                else:
                    # 回退路径: 先删 dst (如果存在) 再 rename, 非原子, 但兼容老 SFTP
                    try:
                        sftp.remove(dst_resolved)
                    except FileNotFoundError:
                        pass
                    sftp.rename(src_resolved, dst_resolved)
        except (OSError, paramiko.SSHException) as exc:
            raise SSHConnectionError(f"重命名远端文件失败: {exc}") from exc

    def open_local_tunnel(
        self,
        remote_port: int,
        *,
        remote_host: str = "127.0.0.1",
        label: str | None = None,
    ) -> "LocalPortForwarder":
        # P3.W1: 隧道一旦建立就持有一个 paramiko Transport 子 channel,
        # transport 死亡时上层会主动 stop 旧隧道再调用本方法新建. 这里只做
        # "前置探活", 确保 transport 仍可开 channel; 不引入 retry, 避免在用户
        # 已经 stop 旧隧道的边界条件下产生孤儿 channel.
        if not self.is_connected:
            self.ensure_alive(reconnect=True)
        return self._open_local_tunnel_once(
            remote_port,
            remote_host=remote_host,
            label=label,
        )

    def _open_local_tunnel_once(
        self,
        remote_port: int,
        *,
        remote_host: str = "127.0.0.1",
        label: str | None = None,
    ) -> "LocalPortForwarder":
        """打开一条本地 -> 远端的 ``direct-tcpip`` 端口转发隧道.

        需要在 [`connect`](src/core/remote/ssh_client.py) 之后调用.
        返回的 [`LocalPortForwarder`](src/core/remote/tunnel.py) 已经处于
        ``running`` 状态, 可通过 ``forwarder.local_port`` 拿到本地随机端口.

        Args:
            remote_port: 远端目标端口 (例如 NapCat WebUI 端口)
            remote_host: 远端目标主机, 默认 ``127.0.0.1`` (远端 loopback)
            label: 日志可读标签, 缺省自动生成

        Returns:
            已启动的 [`LocalPortForwarder`](src/core/remote/tunnel.py); 调用方负责
            在用完后调用 ``stop()`` 释放本地端口.
        """
        # 延迟导入避免与 [`tunnel`](src/core/remote/tunnel.py) 之间的循环依赖
        from .tunnel import LocalPortForwarder

        client = self._require_client()
        transport = client.get_transport()
        if transport is None or not transport.is_active():
            raise SSHConnectionError("SSH transport 不可用, 无法打开端口转发隧道")

        effective_label = label or f"{self.credentials.host}->{remote_host}:{remote_port}"
        forwarder = LocalPortForwarder(
            transport,
            remote_host=remote_host,
            remote_port=remote_port,
            label=effective_label,
        )
        forwarder.start()
        return forwarder

    @property
    def is_connected(self) -> bool:
        """当前是否持有可用 SSH 会话。"""
        if self._client is None:
            return False
        transport = self._client.get_transport()
        return transport is not None and transport.is_active()

    def __enter__(self) -> "SSHClient":
        self.connect()
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        self.close()

    @staticmethod
    def _ensure_paramiko_available() -> None:
        if paramiko is None:
            raise SSHConnectionError("当前环境未安装 paramiko，无法启用远程 SSH 能力")

    def _apply_host_key_policy(self, client: "paramiko.SSHClient") -> None:
        """根据 [`SSHCredentials.host_key_policy`](src/core/remote/models.py) 配置 paramiko policy.

        P4 F5.1 行为:

        - ``"reject"`` / ``"warning"`` / ``"auto_add"``: 与 P3 行为一致.
        - ``"interactive"``: 走 [`InteractiveHostKeyPolicy`](src/core/remote/host_key_policy.py),
          回调由 UI 启动期通过
          [`register_host_key_callback`](src/core/remote/host_key_policy.py) 注入;
          回调缺失时安全兜底为 ``reject_all_callback`` (拒绝所有未知主机), 比无声
          ``AutoAddPolicy`` 更符合 §6.2 安全基线.

        所有政策都会先 ``load_system_host_keys()`` + 再尝试加载用户级
        [`KnownHostsStore`](src/core/remote/host_key_policy.py); 已存在的指纹
        校验由 paramiko 自身完成 (变更指纹会抛 ``BadHostKeyException``,
        本 policy 不参与).
        """
        client.load_system_host_keys()
        # 加载应用级 known_hosts (与系统 ~/.ssh/known_hosts 互不污染)
        try:
            from .host_key_policy import KnownHostsStore, default_known_hosts_path

            user_store = KnownHostsStore(default_known_hosts_path())
            user_store_keys = user_store.load()
            for hostname in user_store_keys.keys():
                # paramiko HostKeys 的 keys() 返回 [host_entry] 列表, 直接合并到 client
                for key_type, key in user_store_keys[hostname].items():
                    client.get_host_keys().add(hostname, key_type, key)
        except Exception as exc:  # noqa: BLE001 - 文件损坏 / 权限不足都不应阻断
            logger.trace(
                f"加载应用级 known_hosts 失败 (忽略): {exc!r}",
                LogType.NETWORK,
                LogSource.CORE,
            )

        policy = self.credentials.host_key_policy
        if policy == "reject":
            client.set_missing_host_key_policy(paramiko.RejectPolicy())
            return
        if policy == "warning":
            client.set_missing_host_key_policy(paramiko.WarningPolicy())
            return
        if policy == "interactive":
            # 延迟 import 避免与 host_key_policy 之间的循环 (paramiko 同时被两边引用)
            from .host_key_policy import (
                InteractiveHostKeyPolicy,
                KnownHostsStore as _KH,
                default_known_hosts_path as _default_path,
                get_registered_callback,
                reject_all_callback,
            )

            callback = get_registered_callback() or reject_all_callback
            interactive = InteractiveHostKeyPolicy(
                callback=callback,
                store=_KH(_default_path()),
                port=self.credentials.port,
            )
            client.set_missing_host_key_policy(interactive)
            return
        # 默认 / "auto_add"
        client.set_missing_host_key_policy(paramiko.AutoAddPolicy())

    def _require_client(self) -> "paramiko.SSHClient":
        if self._client is None:
            raise SSHConnectionError("SSH 尚未连接，请先调用 connect()")
        return self._client

    def _resolve_sftp_path(self, remote_path: str) -> str:
        """将 shell 风格路径转换为 SFTP 可识别的绝对路径。"""
        if remote_path.startswith("$HOME"):
            home_dir = self._get_remote_home_directory()
            suffix = remote_path[len("$HOME") :]
            return f"{home_dir}{suffix}"
        return remote_path

    def _get_remote_home_directory(self) -> str:
        """查询远端用户家目录，并在当前连接内缓存。"""
        if self._remote_home_dir:
            return self._remote_home_dir

        result = self.run('printf %s "$HOME"', check=True)
        home_dir = result.stdout.strip()
        if not home_dir:
            raise SSHConnectionError("无法解析远端 HOME 目录")
        self._remote_home_dir = home_dir
        logger.info(
            f"已解析远端 HOME 目录: host={self.credentials.host}, home={home_dir}",
            LogType.NETWORK,
            LogSource.CORE,
        )
        return home_dir

    @staticmethod
    def _quote_remote_argument(value: str) -> str:
        """为远端 shell 渲染参数。"""
        if value.startswith("$HOME"):
            return f'"{value}"'
        return shlex.quote(value)
