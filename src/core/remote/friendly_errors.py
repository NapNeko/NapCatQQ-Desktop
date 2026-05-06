# -*- coding: utf-8 -*-
"""[`to_friendly`](src/core/remote/friendly_errors.py): 把底层 SSH / socket 异常映射为
**用户可读的中文文案** (P4 F5.4).

设计目标
--------

P3 之前, RemotePage / ServerEditDialog / BotPage 的失败 InfoBar 直接展示
``str(exc)``, 出现 ``AuthenticationException`` / ``BadHostKeyException`` /
``NoValidConnectionsError`` 这种 **paramiko 类名 + 长 traceback** 的字眼,
对普通用户极不友好.

本模块只做一件事:

- 输入: 任意 ``Exception`` 实例
- 输出: 一句 **不带 traceback** 的中文短句, 供 ``error_bar`` / 对话框文案使用

具体规则:

- 优先按异常类型精确匹配 ``FRIENDLY_MESSAGES`` 注册表
- 注册表未命中时, 退化为 ``__cause__`` / ``__context__`` 链上递归查找一次
- 仍未命中时, 返回 ``str(exc)`` 或 ``type(exc).__name__`` (二选一非空)

约束
----

- **不**记录日志, **不**改变异常类型, **不**触碰 GUI; 仅做字符串映射
- 调用方仍应在主入口处用 ``logger.warning(traceback)`` 记原始异常, 不能因此丢调试信息
- 该模块对 ``paramiko`` import 失败要鲁棒: 测试环境可能在没有 paramiko 的容器里跑,
  我们在 import 失败时把 paramiko 相关条目自动跳过, 仅保留 stdlib 异常映射
"""
from __future__ import annotations

# 标准库导入
import socket
from collections.abc import Callable
from typing import Any

# 项目内模块导入
from .errors import (
    RemoteCommandError,
    RemoteDeploymentError,
    RemoteDeploymentInProgressError,
    SSHAuthenticationError,
    SSHConnectionError,
    SSHHostKeyError,
)

try:
    import paramiko  # type: ignore[import-not-found]
except ImportError:  # pragma: no cover - 测试环境鲁棒性
    paramiko = None  # type: ignore[assignment]


_FriendlyHandler = Callable[[Exception], str]


def _format_authentication(exc: Exception) -> str:
    return "用户名或密码错误"


def _format_bad_host_key(exc: Exception) -> str:
    hostname = getattr(exc, "hostname", None) or "目标主机"
    return f"主机指纹与已知记录不匹配: {hostname}; 出于安全考虑已拒绝连接"


def _format_no_valid_connections(exc: Exception) -> str:
    return "无法连接到目标主机, 请检查 IP / 端口 / 防火墙是否放行"


def _format_password_required(exc: Exception) -> str:
    return "私钥已加密, 请提供正确的口令"


def _format_ssh_generic(exc: Exception) -> str:
    text = str(exc).strip()
    if not text:
        return "SSH 协议异常, 请重试或检查网络"
    return f"SSH 协议异常: {text}"


def _format_gai(exc: Exception) -> str:
    return f"无法解析主机名: {exc}"


def _format_refused(exc: Exception) -> str:
    return "目标端口拒绝连接, 请检查 SSH 服务是否启动 / 端口是否正确"


def _format_timeout(exc: Exception) -> str:
    return "连接超时, 请检查网络与防火墙"


def _format_connection_reset(exc: Exception) -> str:
    return "连接被对端重置, 请稍后重试或检查 SSH 服务状态"


def _format_file_not_found(exc: Exception) -> str:
    text = str(exc).strip()
    if text:
        return f"文件不存在: {text}"
    return "文件不存在"


def _format_permission(exc: Exception) -> str:
    text = str(exc).strip()
    if text:
        return f"无访问权限: {text}"
    return "无访问权限"


def _format_remote_command_error(exc: Exception) -> str:
    cmd = getattr(exc, "command", "")
    exit_status = getattr(exc, "exit_status", None)
    stderr = (getattr(exc, "stderr", "") or "").strip()
    parts = ["远端命令执行失败"]
    if exit_status is not None:
        parts.append(f"(退出码 {exit_status})")
    if stderr:
        # 截短到 200 字以避免 InfoBar 撑爆
        compact = stderr if len(stderr) <= 200 else stderr[:200] + "…"
        parts.append(f": {compact}")
    elif cmd:
        compact_cmd = cmd if len(cmd) <= 80 else cmd[:80] + "…"
        parts.append(f": {compact_cmd}")
    return "".join(parts)


def _format_remote_deployment(exc: Exception) -> str:
    stage = getattr(exc, "stage", "") or "部署"
    text = str(exc).strip()
    # RemoteDeploymentError.__init__ 把 stage 也拼进 text 了, 直接用即可
    return text or f"远端 {stage} 失败"


def _format_remote_deployment_in_progress(exc: Exception) -> str:
    return "已有部署任务正在进行, 请等待当前任务结束后再试"


# 注册表: 异常类型 -> 文案生成器
# 顺序敏感: 子类异常应优先于父类被匹配, 故走"isinstance + 第一个命中"
_RAW_REGISTRY: list[tuple[type[BaseException], _FriendlyHandler]] = [
    # ---- NapCatQQ Desktop 自定义远端异常 ----
    (RemoteDeploymentInProgressError, _format_remote_deployment_in_progress),
    (RemoteDeploymentError, _format_remote_deployment),
    (RemoteCommandError, _format_remote_command_error),
    (SSHAuthenticationError, _format_authentication),
    (SSHHostKeyError, _format_bad_host_key),
    (SSHConnectionError, _format_ssh_generic),
    # ---- stdlib 网络异常 ----
    (socket.gaierror, _format_gai),
    (ConnectionRefusedError, _format_refused),
    (ConnectionResetError, _format_connection_reset),
    (TimeoutError, _format_timeout),
    (FileNotFoundError, _format_file_not_found),
    (PermissionError, _format_permission),
]


def _extend_with_paramiko() -> None:
    """运行期把 paramiko 类型条目追加到注册表 (paramiko 缺失时静默跳过).

    paramiko 异常优先级:

    - ``BadHostKeyException`` 必须在 ``SSHException`` 之前命中
    - ``AuthenticationException`` 同样在 ``SSHException`` 之前
    - ``PasswordRequiredException`` (key 加密未给口令) 是
      ``AuthenticationException`` 子类, 需更前
    """
    if paramiko is None:
        return
    # 先把更窄的类型 (Password / BadHostKey / NoValidConnections) 插到注册表头部,
    # 保证 isinstance 命中顺序正确.
    paramiko_entries: list[tuple[type[BaseException], _FriendlyHandler]] = []
    pwd_required = getattr(paramiko, "PasswordRequiredException", None)
    if pwd_required is not None:
        paramiko_entries.append((pwd_required, _format_password_required))
    bad_host = getattr(paramiko, "BadHostKeyException", None)
    if bad_host is not None:
        paramiko_entries.append((bad_host, _format_bad_host_key))
    # paramiko 4.x: NoValidConnectionsError 仅在 paramiko.ssh_exception 子模块暴露,
    # 顶层 ``paramiko`` 命名空间里取不到; 故走子模块兜底
    no_valid: Any = getattr(paramiko, "NoValidConnectionsError", None)
    if no_valid is None:
        try:
            from paramiko.ssh_exception import NoValidConnectionsError as _NVCE
        except ImportError:  # pragma: no cover - paramiko 缺失或子模块改名
            _NVCE = None  # type: ignore[assignment]
        no_valid = _NVCE
    if no_valid is not None:
        paramiko_entries.append((no_valid, _format_no_valid_connections))
    auth = getattr(paramiko, "AuthenticationException", None)
    if auth is not None:
        paramiko_entries.append((auth, _format_authentication))
    ssh_generic = getattr(paramiko, "SSHException", None)
    if ssh_generic is not None:
        paramiko_entries.append((ssh_generic, _format_ssh_generic))
    # paramiko 条目放最前, 让其优先于内建 OSError 等被命中
    _RAW_REGISTRY[0:0] = paramiko_entries


_extend_with_paramiko()


# 暴露给外部以方便测试 / 自定义注册
FRIENDLY_MESSAGES: tuple[tuple[type[BaseException], _FriendlyHandler], ...] = tuple(_RAW_REGISTRY)


def _lookup_handler(exc: BaseException) -> _FriendlyHandler | None:
    """按注册表顺序找第一个能处理该异常的 handler."""
    for exc_type, handler in FRIENDLY_MESSAGES:
        if isinstance(exc, exc_type):
            return handler
    return None


def to_friendly(exc: BaseException) -> str:
    """把异常映射为单行中文用户文案.

    Args:
        exc: 任意异常实例.

    Returns:
        非空字符串. 注册表命中时返回对应模板; 否则尝试 ``__cause__`` / ``__context__``
        链上找一次; 都未命中时返回 ``str(exc)`` 或类名.
    """
    handler = _lookup_handler(exc)
    if handler is not None:
        try:
            text = handler(exc)
        except Exception:  # noqa: BLE001 - 文案生成失败不应再抛
            text = ""
        if text:
            return text

    # 回退: 沿 cause / context 链找一次
    chained: BaseException | None = exc.__cause__ or exc.__context__
    if chained is not None and chained is not exc:
        chained_handler = _lookup_handler(chained)
        if chained_handler is not None:
            try:
                text = chained_handler(chained)
            except Exception:  # noqa: BLE001
                text = ""
            if text:
                return text

    raw = str(exc).strip()
    if raw:
        return raw
    return type(exc).__name__


def to_friendly_with_detail(exc: BaseException) -> tuple[str, str]:
    """返回 ``(headline, detail)`` 二元组.

    - ``headline``: :func:`to_friendly` 输出的简短中文文案
    - ``detail``: ``repr(exc)`` 形式的原始异常字符串, 供"展开详情"用

    UI 展示时通常 ``error_bar(headline)`` 即可; 需要让用户看 traceback 时
    再把 ``detail`` 放到对话框里.
    """
    headline = to_friendly(exc)
    detail = repr(exc)
    return headline, detail


__all__: tuple[str, ...] = (
    "FRIENDLY_MESSAGES",
    "to_friendly",
    "to_friendly_with_detail",
)
