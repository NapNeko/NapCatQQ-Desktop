# -*- coding: utf-8 -*-
"""[`InteractiveHostKeyPolicy`](src/core/remote/host_key_policy.py) (P4 F5.1).

替代 [`paramiko.AutoAddPolicy`](https://docs.paramiko.org/) 的"无声信任"行为, 让
首次连接的服务器主机指纹由用户**显式确认**, 与 §6.2 安全基线对齐.

设计要点
--------

- 上层 SSH 调用可能发生在工作线程 (例如部署 / 启动 Bot 都派给 ``QThreadPool``).
  ``MissingHostKeyPolicy.missing_host_key`` 在 worker 线程被调用; 不能直接弹 UI.
  本模块仅定义**抽象的决策回调** :class:`HostKeyConfirmCallback`, 由调用方
  (``SSHClient`` / 测试) 提供; 真实 UI 弹窗由
  [`HostKeyConfirmDialog`](src/ui/components/host_key_confirm_dialog.py) 通过
  ``QMetaObject.invokeMethod`` 跨线程同步阻塞实现 (P4 W1 后续子任务).
- 持久化使用 paramiko 自带 [`paramiko.HostKeys`](https://docs.paramiko.org/),
  落盘格式与 OpenSSH ``known_hosts`` 兼容; 用户也可手动用 ``ssh-keyscan`` 预填该文件.
- 主机指纹**变化**走 paramiko 默认 ``BadHostKeyException`` 路径, **不**经过本 policy
  (paramiko 在调用 policy 之前已经自己抛出); 调用方应在 SSHClient 包装层捕获后
  弹**红色警告对话框**, 不允许用户在此处一键覆盖.
"""
from __future__ import annotations

# 标准库导入
import base64
import enum
import hashlib
import threading
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import paramiko

try:
    import paramiko  # type: ignore[no-redef]
except ImportError:  # pragma: no cover - paramiko 缺失时纯靠类型注解
    paramiko = None  # type: ignore[assignment]


# ==================== 数据模型 ====================
class HostKeyDecision(enum.Enum):
    """用户对主机指纹的决策."""

    #: 信任并保存到 ``known_hosts`` (下次连接不再弹窗); **首次连接** (未知主机) 路径
    TRUST_SAVE = "trust_save"
    #: 仅本次连接信任, 不写盘 (下次仍弹窗); 首次连接路径
    TRUST_ONCE = "trust_once"
    #: 拒绝, 中断 SSH 握手
    REJECT = "reject"
    #: 信任并**替换** known_hosts 中已有条目; **变更** (BadHostKeyException) 路径专用,
    #: 调用方需先 remove 旧 entry 再 add 新 entry, 然后重连一次
    TRUST_REPLACE = "trust_replace"


@dataclass(slots=True, frozen=True)
class HostKeyPrompt:
    """传递给 UI 决策回调的纯数据快照.

    使用 ``frozen=True`` 让回调实现可以放心持有 / 跨线程传递.
    """

    hostname: str
    port: int
    key_type: str
    fingerprint_sha256: str
    """OpenSSH 风格的 SHA256 指纹 (``SHA256:<base64-no-padding>``)."""

    fingerprint_md5: str = ""
    """OpenSSH 历史风格 MD5 指纹 (``aa:bb:cc:...``); 仅供老用户视觉对照, 不强制提供."""

    previous_key_type: str = ""
    """变更场景下原 known_hosts 中的密钥类型; 首次连接为空."""

    previous_fingerprint_sha256: str = ""
    """变更场景下原 known_hosts 中的 SHA256 指纹; 首次连接为空.

    UI ``is_warning=True`` 时若该字段非空, 应在对话框中展示"原指纹 vs 新指纹"
    对比, 让用户明确感知到主机指纹**变更**而非首次连接.
    """


HostKeyConfirmCallback = Callable[..., HostKeyDecision]
"""决策回调签名: 同步返回用户决策.

标准调用形式::

    callback(prompt: HostKeyPrompt) -> HostKeyDecision                # 首次连接
    callback(prompt: HostKeyPrompt, *, is_warning=True) -> HostKeyDecision  # 指纹变更

实现方应在主线程内同步取得用户选择 (Qt 端用 ``invokeMethod(BlockingQueuedConnection)``);
回调可能在工作线程被调用, 需自行处理跨线程.

``is_warning=True`` 时 UI 应展示红色警告对话框 (指纹变更路径), 提供
``TRUST_REPLACE`` / ``REJECT`` 选项; 缺省 ``False`` 时展示首次连接对话框.
"""


# ==================== 指纹计算 ====================
def compute_sha256_fingerprint(key) -> str:
    """计算 OpenSSH 风格的 SHA256 指纹.

    输出格式 ``SHA256:<base64-no-padding>``, 与 ``ssh -o FingerprintHash=sha256``
    输出一致, 也是 OpenSSH 8.x+ 默认格式.

    Args:
        key: paramiko ``PKey`` 实例 (任何子类: RSAKey / Ed25519Key / EcdsaKey).

    Returns:
        完整指纹字符串 (含 ``SHA256:`` 前缀).
    """
    raw = key.asbytes() if hasattr(key, "asbytes") else bytes(key)
    digest = hashlib.sha256(raw).digest()
    b64 = base64.b64encode(digest).decode("ascii").rstrip("=")
    return f"SHA256:{b64}"


def compute_md5_fingerprint(key) -> str:
    """计算 OpenSSH 历史风格 MD5 指纹 ``aa:bb:cc:...``.

    仅供用户与老资料/工具核对; 新场景应优先用
    :func:`compute_sha256_fingerprint`.
    """
    raw = key.asbytes() if hasattr(key, "asbytes") else bytes(key)
    digest = hashlib.md5(raw).digest()  # noqa: S324 - SSH 标准定义就是 MD5
    return ":".join(f"{b:02x}" for b in digest)


# ==================== known_hosts 持久化 ====================
def default_known_hosts_path() -> Path:
    """返回应用持久化 ``known_hosts`` 路径.

    路径: ``<resolve_app_data_path()>/ssh/known_hosts``.
    与 OpenSSH 用户级 ``~/.ssh/known_hosts`` 解耦, 避免污染用户 OpenSSH 状态;
    用户若需要导入 OpenSSH 已记录主机, 可用 :meth:`KnownHostsStore.import_from_openssh`.
    """
    # 延迟 import 避免与 src.core.runtime.paths -> creart 等初始化序冲突
    from src.core.platform.app_paths import resolve_app_data_path

    return resolve_app_data_path() / "ssh" / "known_hosts"


@dataclass(slots=True)
class KnownHostsStore:
    """``paramiko.HostKeys`` 持久化的薄包装.

    提供 ``add`` / ``contains`` / ``save`` / ``load`` / ``import_from_openssh``
    五个动作; 内部 lock 保证并发 add / save 不会互相打架.
    """

    path: Path
    _host_keys: "paramiko.HostKeys | None" = field(default=None, init=False, repr=False)
    _lock: threading.RLock = field(default_factory=threading.RLock, init=False, repr=False)

    def _require_paramiko(self) -> None:
        if paramiko is None:
            raise RuntimeError("paramiko 未安装, 无法操作 known_hosts")

    def load(self) -> "paramiko.HostKeys":
        """加载磁盘上的 known_hosts; 不存在或损坏时返回空集.

        多次调用幂等: 第二次返回同一个 ``HostKeys`` 实例.

        ``paramiko.HostKeys.load`` 在某行 base64 不合法时会抛
        ``paramiko.hostkeys.InvalidHostKey`` (不是 ``SSHException`` 子类), 这里
        统一吞掉, 避免 SSH 入口因 known_hosts 损坏整体失败.
        """
        self._require_paramiko()
        with self._lock:
            if self._host_keys is None:
                self._host_keys = paramiko.HostKeys()
                if self.path.exists():
                    try:
                        self._host_keys.load(str(self.path))
                    except Exception:  # noqa: BLE001 - 文件损坏不应阻断 SSH 流程
                        # paramiko 历史版本会抛 SSHException; 4.x 抛 InvalidHostKey;
                        # 用宽泛 Exception 兜底, 任何 known_hosts 损坏都退化为空集
                        self._host_keys = paramiko.HostKeys()
            return self._host_keys

    def contains(self, hostname: str, port: int = 22) -> bool:
        """``hostname[:port]`` 是否已在 known_hosts 中有任意 key 类型条目."""
        host_keys = self.load()
        return host_keys.lookup(_format_host_entry(hostname, port)) is not None

    def add(self, hostname: str, port: int, key) -> None:
        """添加 ``(hostname, port)`` 的 key 到内存集合 (不立即落盘)."""
        host_keys = self.load()
        with self._lock:
            host_keys.add(_format_host_entry(hostname, port), key.get_name(), key)

    def remove(self, hostname: str, port: int = 22) -> bool:
        """从内存集合中移除 ``(hostname, port)`` 的所有 key (不立即落盘).

        Returns:
            ``True`` 表示移除了至少一条; ``False`` 表示没找到匹配条目.
        """
        host_keys = self.load()
        entry = _format_host_entry(hostname, port)
        with self._lock:
            # paramiko HostKeys 内部是 list, 直接遍历删除
            removed = False
            i = 0
            while i < len(host_keys._entries):
                e = host_keys._entries[i]
                if entry in e.hostnames:
                    host_keys._entries.pop(i)
                    removed = True
                    continue
                i += 1
            return removed

    def lookup_fingerprint(self, hostname: str, port: int = 22) -> tuple[str, str]:
        """查询 ``(hostname, port)`` 已记录的 ``(key_type, sha256_fingerprint)``.

        Returns:
            ``(key_type, fingerprint_sha256)``; 找不到时返回 ``("", "")``.
            多 key 类型同主机时返回 paramiko ``lookup`` 命中的第一条.
        """
        host_keys = self.load()
        entry = _format_host_entry(hostname, port)
        bag = host_keys.lookup(entry)
        if not bag:
            return "", ""
        # bag 是 dict[key_type, key]; 取第一对
        try:
            key_type = next(iter(bag.keys()))
            key = bag[key_type]
        except StopIteration:
            return "", ""
        return key_type, compute_sha256_fingerprint(key)

    def save(self) -> None:
        """落盘当前内存集合; 父目录不存在自动创建."""
        if self._host_keys is None:
            return
        with self._lock:
            self.path.parent.mkdir(parents=True, exist_ok=True)
            self._host_keys.save(str(self.path))

    def import_from_openssh(self) -> int:
        """把用户级 ``~/.ssh/known_hosts`` 合并到当前集合.

        只读旧文件, 不修改它; 写入仅落到本应用的 known_hosts 路径 (需调用方再 save).

        Returns:
            导入到的 host 数量 (重复条目不重复计数).
        """
        self._require_paramiko()
        legacy = Path.home() / ".ssh" / "known_hosts"
        if not legacy.exists():
            return 0
        host_keys = self.load()
        before = len(list(host_keys.keys()))
        try:
            host_keys.load(str(legacy))
        except (OSError, paramiko.SSHException):
            return 0
        after = len(list(host_keys.keys()))
        return max(0, after - before)


def _format_host_entry(hostname: str, port: int) -> str:
    """OpenSSH known_hosts 中非 22 端口的条目写作 ``[host]:port``."""
    if port == 22 or port <= 0:
        return hostname
    return f"[{hostname}]:{port}"


def _strip_host_entry(host_entry: str) -> str:
    """从 paramiko 传入的 ``[host]:port`` 中提取 bare host; 已是 bare 时原样返回.

    paramiko ``SSHClient.connect`` 在 ``port != 22`` 时, 会把
    ``server_hostkey_name`` 预先格式化为 ``[host]:port`` 再传给
    ``MissingHostKeyPolicy.missing_host_key``. 我们内部 store 与 UI prompt
    的契约都是 bare host + 独立 port, 必须先剥离, 否则 ``_format_host_entry``
    会二次包装出 ``[[host]:port]:port`` 之类的废条目, 下次 lookup 永远命中不了.
    """
    if host_entry.startswith("[") and "]:" in host_entry:
        end = host_entry.index("]:")
        # 只取 ``[`` 与第一个 ``]:`` 之间的内容, 端口段直接丢
        return host_entry[1:end]
    return host_entry


# ==================== paramiko policy ====================
class _BasePolicy:
    """``paramiko.MissingHostKeyPolicy`` 兼容父类.

    paramiko 缺失时也能定义类 (避免 import 期失败); 真实运行依赖 paramiko 时
    会通过 ``__init_subclass__`` 钩到 ``paramiko.MissingHostKeyPolicy``.
    """


if paramiko is not None:
    _BasePolicy = paramiko.MissingHostKeyPolicy  # type: ignore[misc, assignment]


class InteractiveHostKeyPolicy(_BasePolicy):  # type: ignore[misc]
    """让用户对未知主机指纹做交互式决策的 paramiko policy.

    Args:
        callback: 决策回调, 必须**同步**返回 :class:`HostKeyDecision`.
            UI 实现负责跨线程派发到主线程同步等待用户操作.
        store: ``known_hosts`` 持久化 store; 缺省走 :func:`default_known_hosts_path`.
        port: SSH 端口, 缺省 22; ``missing_host_key`` 不携带端口, 故由
            :class:`SSHClient` 在每次 connect 前注入实例属性.

    用法 (在 ``SSHClient.connect`` 中)::

        policy = InteractiveHostKeyPolicy(callback=ui_callback, port=cred.port)
        client.set_missing_host_key_policy(policy)
        # ...
    """

    def __init__(
        self,
        *,
        callback: HostKeyConfirmCallback,
        store: KnownHostsStore | None = None,
        port: int = 22,
    ) -> None:
        if paramiko is None:  # pragma: no cover - paramiko 缺失时不应被实例化
            raise RuntimeError("paramiko 未安装, 无法构造 InteractiveHostKeyPolicy")
        super().__init__()
        self._callback = callback
        self._store = store if store is not None else KnownHostsStore(default_known_hosts_path())
        self._port = port

    @property
    def store(self) -> KnownHostsStore:
        """暴露内部 KnownHostsStore (主要用于测试)."""
        return self._store

    def set_port(self, port: int) -> None:
        """SSHClient 在调用 ``client.connect`` 前注入实际端口."""
        self._port = port

    # paramiko 接口
    def missing_host_key(self, client, hostname: str, key) -> None:  # noqa: D401
        # paramiko 在 port != 22 时把 hostname 预先包装成 ``[host]:port``,
        # 我们 store / prompt 的契约是 bare host + 独立 port, 这里先剥离一次,
        # 之后所有写入都基于 bare host, 由 ``_format_host_entry`` 统一格式化.
        bare_hostname = _strip_host_entry(hostname)

        prompt = HostKeyPrompt(
            hostname=bare_hostname,
            port=self._port,
            key_type=key.get_name(),
            fingerprint_sha256=compute_sha256_fingerprint(key),
            fingerprint_md5=compute_md5_fingerprint(key),
        )

        try:
            decision = self._callback(prompt)
        except Exception as exc:  # noqa: BLE001 - 任何回调异常都视为拒绝
            raise paramiko.SSHException(
                f"未知主机指纹确认对话框异常, 已拒绝连接: {exc!r}"
            ) from exc

        if decision is HostKeyDecision.REJECT:
            raise paramiko.SSHException(
                f"用户拒绝信任主机指纹: {bare_hostname} ({prompt.fingerprint_sha256})"
            )

        # TRUST_REPLACE 仅 BadHostKeyException 路径用; 但 callback 实现误返
        # 在 missing_host_key 时把它兼容当 TRUST_SAVE 处理, 避免崩溃.
        if decision in (HostKeyDecision.TRUST_SAVE, HostKeyDecision.TRUST_REPLACE):
            # 注入 client 内存集合 + 持久化
            entry = _format_host_entry(bare_hostname, self._port)
            client.get_host_keys().add(entry, key.get_name(), key)
            self._store.add(bare_hostname, self._port, key)
            try:
                self._store.save()
            except OSError:
                # 写盘失败不应阻断本次连接, 但下次仍会弹窗
                pass
            return

        # TRUST_ONCE: 既不持久化也不写入 client.host_keys, 让 paramiko 走默认通过路径
        # paramiko AutoAddPolicy 内部就是只 add 到 client.host_keys 内存集合;
        # 这里**完全不 add**, 让本次 SSHClient 实例在后续会话中再来一次 missing_host_key
        # (但 SSHClient 通常一连接就用一段时间, 实际效果与 once 语义一致).
        return


# ==================== 全局回调注册表 ====================
# 让 [`SSHClient`](src/core/remote/ssh_client.py) 在 ``host_key_policy="interactive"``
# 下能拿到 UI 注入的回调; UI 启动期 (`bootstrap_host_key_dialog`) 注册一次即可.
_REGISTERED_CALLBACK: HostKeyConfirmCallback | None = None
_REGISTRY_LOCK = threading.RLock()


def register_host_key_callback(callback: HostKeyConfirmCallback | None) -> None:
    """注册全局 host key 决策回调.

    UI 启动期 (例如 ``MainWindow.__init__``) 调用一次, 把 GUI 弹窗逻辑注入到
    SSH 层. 传入 ``None`` 表示清空 (主要用于测试 teardown).

    Args:
        callback: 决策回调; **必须**是线程安全且**同步阻塞**等待用户决策的实现.
    """
    global _REGISTERED_CALLBACK
    with _REGISTRY_LOCK:
        _REGISTERED_CALLBACK = callback


def get_registered_callback() -> HostKeyConfirmCallback | None:
    """获取当前注册的 host key 回调; 未注册时返回 ``None``."""
    with _REGISTRY_LOCK:
        return _REGISTERED_CALLBACK


def reject_all_callback(prompt: HostKeyPrompt, **kwargs) -> HostKeyDecision:
    """安全兜底回调: 任何未知主机一律拒绝.

    无 UI 上下文 (如冒烟测试 / 后端脚本) 时作为默认值, 比 ``AutoAddPolicy``
    (无声信任) 更符合 §6.2 安全基线.
    """
    del prompt, kwargs
    return HostKeyDecision.REJECT


__all__: tuple[str, ...] = (
    "HostKeyDecision",
    "HostKeyPrompt",
    "HostKeyConfirmCallback",
    "compute_sha256_fingerprint",
    "compute_md5_fingerprint",
    "default_known_hosts_path",
    "KnownHostsStore",
    "InteractiveHostKeyPolicy",
    "register_host_key_callback",
    "get_registered_callback",
    "reject_all_callback",
)
