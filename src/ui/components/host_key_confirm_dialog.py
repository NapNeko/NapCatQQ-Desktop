# -*- coding: utf-8 -*-
"""[`HostKeyConfirmDialog`](src/ui/components/host_key_confirm_dialog.py) (P4 F5.1).

首次连接未知主机指纹时弹出的交互式确认对话框, 提供三选项:

- "信任并保存": 写入应用级 known_hosts, 下次不再弹窗
- "仅本次": 本次握手通过, 下次仍弹窗
- "拒绝": 中断握手

跨线程桥
--------

paramiko 的 ``MissingHostKeyPolicy.missing_host_key`` 在 SSH worker 线程被调用,
而 Qt 弹窗必须在主线程. 本模块通过
[`HostKeyDialogBridge`](src/ui/components/host_key_confirm_dialog.py) 用 Qt 信号 +
``threading.Event`` 把工作线程"同步阻塞等待主线程弹窗"的语义实现:

1. SSH worker 调用 :func:`prompt_host_key_decision_blocking`;
2. 该函数 emit ``request_signal`` 把 prompt 投到主线程;
3. 主线程槽函数 ``_show_dialog`` 弹 ``HostKeyConfirmDialog`` 并写回结果 + 唤醒 Event;
4. SSH worker 唤醒后读到结果, 返回给 paramiko.

启动期注册
----------

应用启动时 (``MainWindow.__init__`` 末尾或 ``main.py``) 调用一次
:func:`bootstrap_host_key_dialog`, 把桥的 ``prompt`` 方法注册为
[`register_host_key_callback`](src/core/remote/host_key_policy.py); 之后任何
``host_key_policy="interactive"`` 的 SSH 连接都会自动走这条 UI 通路.
"""
from __future__ import annotations

# 标准库导入
import threading
from typing import TYPE_CHECKING

# 第三方库导入
from PySide6.QtCore import (
    QCoreApplication,
    QMetaObject,
    QObject,
    Qt,
    QThread,
    Signal,
    Slot,
)
from PySide6.QtGui import QFont, QGuiApplication
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    FluentIcon,
    MessageBoxBase,
    PrimaryPushButton,
    PushButton,
    StrongBodyLabel,
    SubtitleLabel,
    ToolButton,
    ToolTipFilter,
)

# 项目内模块导入
from src.core.logging import LogSource, LogType, logger
from src.core.remote.host_key_policy import (
    HostKeyDecision,
    HostKeyPrompt,
    register_host_key_callback,
)

if TYPE_CHECKING:
    pass


# ==================== 默认超时 ====================
#: 工作线程等待用户决策的最大时间; 超时直接 REJECT, 防止无人响应 (锁屏 / 崩溃) 导致
#: SSH worker 永久挂起.
_DEFAULT_PROMPT_TIMEOUT_SECONDS: float = 120.0


# ==================== 对话框 ====================
class HostKeyConfirmDialog(MessageBoxBase):
    """首次连接未知主机时弹出的指纹确认对话框.

    Args:
        prompt: 待确认的主机指纹快照.
        parent: 父级窗口; 缺省 None (会以应用顶层为父).
        is_warning: 已知主机指纹**变化**时设为 True; UI 会用红色标题 + 默认拒绝.

    使用方式 (主线程内同步)::

        dialog = HostKeyConfirmDialog(prompt, parent=main_window)
        if dialog.exec():
            decision = dialog.decision()
        else:
            decision = HostKeyDecision.REJECT
    """

    def __init__(
        self,
        prompt: HostKeyPrompt,
        *,
        parent: QWidget | None = None,
        is_warning: bool = False,
    ) -> None:
        super().__init__(parent=parent)
        self._prompt = prompt
        self._is_warning = is_warning
        self._decision = HostKeyDecision.REJECT  # 关闭对话框默认值
        self._setup_ui()
        self._wire_buttons()
        self.widget.setMinimumSize(500, 320)

    # ==================== 公共接口 ====================
    def decision(self) -> HostKeyDecision:
        """返回用户最终决策; 若未点任何按钮直接关闭则返回 REJECT."""
        return self._decision

    # ==================== UI ====================
    def _setup_ui(self) -> None:
        if self._is_warning:
            title_text = "⚠ 主机指纹变化警告"
            caption_text = (
                f"已知主机 {self._prompt.hostname}:{self._prompt.port} 的指纹已变化, "
                "可能是 SSH 服务器重装, 也可能是中间人攻击, 请谨慎处理."
            )
        else:
            title_text = "首次连接 — 主机指纹确认"
            caption_text = (
                f"这是你第一次连接 {self._prompt.hostname}:{self._prompt.port}, "
                "请确认下方主机指纹是否与服务器一致."
            )

        self.title_label = SubtitleLabel(title_text, self)
        self.caption_label = CaptionLabel(caption_text, self)
        self.caption_label.setWordWrap(True)
        if self._is_warning:
            # 仅警告路径用红色; 正常首次连接保持默认色
            self.title_label.setStyleSheet("color: #C42B1C;")

        # 指纹展示区
        info_widget = QWidget(self)
        info_layout = QVBoxLayout(info_widget)
        info_layout.setContentsMargins(0, 0, 0, 0)
        info_layout.setSpacing(6)

        info_layout.addWidget(self._build_kv_row("主机", f"{self._prompt.hostname}:{self._prompt.port}"))

        has_previous = bool(self._prompt.previous_fingerprint_sha256)
        if self._is_warning and has_previous:
            # 主机指纹变化路径: 显示新旧对比, 让用户感知变化幅度
            info_layout.addWidget(
                self._build_kv_row(
                    "原密钥类型",
                    self._prompt.previous_key_type or "-",
                )
            )
            info_layout.addWidget(
                self._build_kv_row(
                    "原 SHA256",
                    self._prompt.previous_fingerprint_sha256,
                    mono=True,
                )
            )
            info_layout.addWidget(
                self._build_kv_row(
                    "新密钥类型",
                    self._prompt.key_type,
                )
            )
            info_layout.addWidget(
                self._build_kv_row(
                    "新 SHA256",
                    self._prompt.fingerprint_sha256,
                    mono=True,
                )
            )
        else:
            # 首次连接路径: 只显示新指纹
            info_layout.addWidget(self._build_kv_row("密钥类型", self._prompt.key_type))
            info_layout.addWidget(self._build_kv_row("SHA256", self._prompt.fingerprint_sha256, mono=True))
            if self._prompt.fingerprint_md5:
                info_layout.addWidget(self._build_kv_row("MD5", self._prompt.fingerprint_md5, mono=True))

        # 安全提示 + 一键复制测试指令
        # 提示行结构: [说明文字...][复制按钮]; 文字与按钮挤在同一行,
        # 按钮 32x32, 走 ToolTipFilter 提示 "复制测试指令".
        hint_widget = QWidget(self)
        hint_layout = QHBoxLayout(hint_widget)
        hint_layout.setContentsMargins(0, 0, 0, 0)
        hint_layout.setSpacing(8)

        hint_label = CaptionLabel(
            "提示: 你可以在服务器上运行 "
            "ssh-keygen -lf /etc/ssh/ssh_host_*_key.pub "
            "获取真实主机指纹后比对.",
            hint_widget,
        )
        hint_label.setWordWrap(True)
        hint_label.setStyleSheet("color: #6b7280;")
        hint_layout.addWidget(hint_label, 1, Qt.AlignmentFlag.AlignTop)

        self._copy_command_btn = ToolButton(FluentIcon.COPY, hint_widget)
        self._copy_command_btn.setFixedSize(30, 30)
        self._copy_command_btn.setToolTip("复制测试指令到剪贴板")
        self._copy_command_btn.setToolTipDuration(1500)
        self._copy_command_btn.installEventFilter(
            ToolTipFilter(self._copy_command_btn, showDelay=300)
        )
        self._copy_command_btn.clicked.connect(self._on_copy_command)
        hint_layout.addWidget(
            self._copy_command_btn, 0, Qt.AlignmentFlag.AlignTop | Qt.AlignmentFlag.AlignRight
        )

        # 装入对话框 layout
        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addWidget(self.caption_label)
        self.viewLayout.addSpacing(6)
        self.viewLayout.addWidget(info_widget)
        self.viewLayout.addSpacing(6)
        self.viewLayout.addWidget(hint_widget)

    def _build_kv_row(self, label_text: str, value_text: str, *, mono: bool = False) -> QWidget:
        """构造一行 ``label: value`` 等宽对齐的展示."""
        row = QWidget(self)
        layout = QHBoxLayout(row)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(8)

        label = StrongBodyLabel(f"{label_text}:", row)
        label.setFixedWidth(78)
        label.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        layout.addWidget(label, 0, Qt.AlignmentFlag.AlignVCenter)

        value = BodyLabel(value_text, row)
        if mono:
            font = QFont("Consolas", 10)
            font.setStyleHint(QFont.StyleHint.Monospace)
            value.setFont(font)
        value.setWordWrap(True)
        value.setTextInteractionFlags(Qt.TextInteractionFlag.TextSelectableByMouse)
        layout.addWidget(value, 1, Qt.AlignmentFlag.AlignVCenter)

        return row

    def _on_copy_command(self) -> None:
        """把"提示"行里的 ssh-keygen 测试指令复制到剪贴板.

        指令本身是个静态字符串, 与提示文案中保持一致; 复制后用户在远端 shell
        粘贴执行就能拿到真实指纹做比对.
        """
        command = "ssh-keygen -lf /etc/ssh/ssh_host_*_key.pub"
        clipboard = QGuiApplication.clipboard()
        if clipboard is not None:
            clipboard.setText(command)
        # 给用户一个即时反馈: 改 tooltip 文案 + 不阻断当前对话框
        self._copy_command_btn.setToolTip("已复制到剪贴板")

    def _wire_buttons(self) -> None:
        """重写 MessageBoxBase 的 yes/cancel 按钮以承载多选语义.

        ``MessageBoxBase`` 默认有 ``yesButton`` (yes) + ``cancelButton`` (no).
        - **首次连接** (``is_warning=False``): 三选 - TRUST_SAVE / TRUST_ONCE / REJECT
        - **变更警告** (``is_warning=True``): 二选 - TRUST_REPLACE / REJECT;
          "仅本次" 在变更场景语义无意义 (paramiko 已 fail, 无法绕开), 故隐藏.
        """
        self.cancelButton.setText("拒绝")

        if self._is_warning:
            # 变更警告路径: 按钮文案改为 "信任并替换" + 红色, 不显示 "仅本次"
            self.yesButton.setText("信任并替换")
            self.yesButton.setStyleSheet(
                "QPushButton { background-color: #C42B1C; color: white; }"
            )
            self._once_button = None
        else:
            # 首次连接路径: 维持原"信任并保存" + 插入 "仅本次"
            self.yesButton.setText("信任并保存")
            once_btn = PushButton("仅本次", self.buttonGroup)
            # MessageBoxBase 的 buttonLayout 是 QHBoxLayout, 在 yesButton 前插一个
            self.buttonLayout.insertWidget(self.buttonLayout.count() - 1, once_btn)
            self._once_button = once_btn
            once_btn.clicked.connect(self._on_trust_once)

        # yes/reject 共通接线
        self.yesButton.clicked.connect(self._on_trust_yes)
        self.cancelButton.clicked.connect(self._on_reject)

    def _on_trust_yes(self) -> None:
        """yes 按钮: 首次连接返 TRUST_SAVE, 变更警告返 TRUST_REPLACE."""
        if self._is_warning:
            self._decision = HostKeyDecision.TRUST_REPLACE
        else:
            self._decision = HostKeyDecision.TRUST_SAVE
        # MessageBoxBase 的 yesButton 默认 accept(); 不重复 accept

    def _on_trust_once(self) -> None:
        self._decision = HostKeyDecision.TRUST_ONCE
        # 手动 accept, 让 exec() 返回 truthy
        self.accept()

    def _on_reject(self) -> None:
        self._decision = HostKeyDecision.REJECT
        # cancelButton 默认 reject(); 不重复


# ==================== 跨线程桥 ====================
class HostKeyDialogBridge(QObject):
    """工作线程 → 主线程的同步桥.

    Qt 在跨线程 ``Signal.emit`` 时若信号订阅者位于不同线程, 默认连接为
    ``Qt.QueuedConnection`` (异步). 我们额外用 ``threading.Event`` 让工作线程
    阻塞等待主线程的弹窗结果.

    实例必须在主线程构造 (``thread() == QApplication.instance().thread()``);
    ``prompt`` 方法可以在任意线程调用.
    """

    # 内部信号: prompt -> main thread 槽
    _request_signal = Signal(object)

    def __init__(self, *, parent: QObject | None = None, timeout_seconds: float = _DEFAULT_PROMPT_TIMEOUT_SECONDS) -> None:
        super().__init__(parent)
        self._timeout = timeout_seconds
        self._lock = threading.Lock()
        self._event = threading.Event()
        self._result: HostKeyDecision = HostKeyDecision.REJECT
        self._pending_prompt: HostKeyPrompt | None = None
        self._is_warning_for_pending: bool = False
        self._request_signal.connect(self._on_request, Qt.ConnectionType.QueuedConnection)

    @Slot(object)
    def _on_request(self, _payload: object) -> None:
        """主线程槽: 弹 ``HostKeyConfirmDialog`` 同步阻塞用户决策, 然后唤醒 Event."""
        prompt = self._pending_prompt
        is_warning = self._is_warning_for_pending
        if prompt is None:
            self._event.set()
            return

        try:
            parent = self._resolve_parent_widget()
            dialog = HostKeyConfirmDialog(prompt, parent=parent, is_warning=is_warning)
            if dialog.exec():
                self._result = dialog.decision()
            else:
                self._result = HostKeyDecision.REJECT
        except Exception as exc:  # noqa: BLE001 - 任何 UI 异常都视为拒绝
            logger.warning(
                f"HostKeyConfirmDialog 异常, 默认拒绝: {exc!r}",
                LogType.NETWORK,
                LogSource.UI,
            )
            self._result = HostKeyDecision.REJECT
        finally:
            self._event.set()

    def prompt(self, prompt: HostKeyPrompt, *, is_warning: bool = False) -> HostKeyDecision:
        """工作线程同步调用: 投递到主线程弹窗, 阻塞等待用户决策.

        Args:
            prompt: 待确认的主机指纹快照.
            is_warning: 已知指纹变化警告路径; 默认 False (首次连接).

        Returns:
            用户决策; 超时 / 异常返回 ``HostKeyDecision.REJECT``.
        """
        # 用 lock 串行化, 避免两个 worker 同时弹窗导致 UI 混乱
        with self._lock:
            # 同线程优化: 已经在主线程时直接弹, 避免事件循环重入
            if self._is_main_thread():
                try:
                    parent = self._resolve_parent_widget()
                    dialog = HostKeyConfirmDialog(prompt, parent=parent, is_warning=is_warning)
                    if dialog.exec():
                        return dialog.decision()
                    return HostKeyDecision.REJECT
                except Exception as exc:  # noqa: BLE001
                    logger.warning(
                        f"HostKeyConfirmDialog 主线程同步弹窗异常: {exc!r}",
                        LogType.NETWORK,
                        LogSource.UI,
                    )
                    return HostKeyDecision.REJECT

            # 跨线程: 投到主线程, 阻塞等待
            self._pending_prompt = prompt
            self._is_warning_for_pending = is_warning
            self._result = HostKeyDecision.REJECT
            self._event.clear()
            self._request_signal.emit(prompt)
            if not self._event.wait(timeout=self._timeout):
                logger.warning(
                    f"主线程未在 {self._timeout:.0f}s 内响应主机指纹确认, 默认拒绝: "
                    f"{prompt.hostname}:{prompt.port}",
                    LogType.NETWORK,
                    LogSource.UI,
                )
                return HostKeyDecision.REJECT
            return self._result

    # ==================== 内部 ====================
    @staticmethod
    def _is_main_thread() -> bool:
        app = QCoreApplication.instance()
        if app is None:
            # 无 QApplication 上下文 (单测): 视为同线程, 让调用方触发 timeout 路径
            return True
        return QThread.currentThread() is app.thread()

    @staticmethod
    def _resolve_parent_widget() -> QWidget | None:
        """尝试用 creart 拿 MainWindow 作为父级; 拿不到时返回 None."""
        try:
            from creart import it
            from src.ui.window.main_window.window import MainWindow

            return it(MainWindow)
        except Exception:  # noqa: BLE001 - MainWindow 未初始化时容忍
            return None


# ==================== 启动期 bootstrap ====================
_GLOBAL_BRIDGE: HostKeyDialogBridge | None = None


def bootstrap_host_key_dialog() -> HostKeyDialogBridge:
    """启动期把 :class:`HostKeyDialogBridge` 注册为全局 host key 决策回调.

    幂等: 多次调用返回同一个 bridge 实例.

    Returns:
        当前生效的 bridge (供调用方做 unit test 注入或诊断).
    """
    global _GLOBAL_BRIDGE
    if _GLOBAL_BRIDGE is None:
        _GLOBAL_BRIDGE = HostKeyDialogBridge()
        register_host_key_callback(_GLOBAL_BRIDGE.prompt)
    return _GLOBAL_BRIDGE


def reset_host_key_dialog_for_test() -> None:
    """测试 teardown 用: 清空全局 bridge + 注销回调."""
    global _GLOBAL_BRIDGE
    register_host_key_callback(None)
    _GLOBAL_BRIDGE = None


__all__: tuple[str, ...] = (
    "HostKeyConfirmDialog",
    "HostKeyDialogBridge",
    "bootstrap_host_key_dialog",
    "reset_host_key_dialog_for_test",
)
