# -*- coding: utf-8 -*-
"""远程服务器管理页面 (v2 架构) . 

对应 [`docs/general/remote_ssh_plan.md`](../../../../../../docs/general/remote_ssh_plan.md) §3 与 §10. 
v2 = VS Code Remote 模型: 本地 UI 透明代理远端 NapCat Core,
本页负责服务器档案的 CRUD 与连接测试; 部署 / Bot 绑定将在 P1 / P2 阶段加入. 
"""

from __future__ import annotations

from abc import ABC
from typing import TYPE_CHECKING

from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module, it
from PySide6.QtCore import QSize, Qt, QThreadPool, QTimer, Signal
from PySide6.QtGui import QResizeEvent, QShowEvent
from PySide6.QtWidgets import (
    QFormLayout,
    QVBoxLayout,
    QWidget,
)
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    CheckBox,
    FlowLayout,
    HeaderCardWidget,
    InfoBadge,
    MessageBoxBase,
    PrimaryToolButton,
    ProgressBar,
    ScrollArea,
    StrongBodyLabel,
    TitleLabel,
    ToolButton,
    ToolTipFilter,
    TransparentToolButton,
    FluentIcon as FI,
)

from src.core.config import cfg
from src.core.remote import BackendFlavor, DeploymentState, ServerManager, ServerProfile
from src.core.remote.thread_pool import remote_ssh_pool
from src.ui.common.style_sheet import PageStyleSheet
from src.ui.components.info_bar import error_bar, info_bar, success_bar
from src.ui.components.message_box import AskBox

from .connection_tester import ConnectionTester
from .deployment_console import DeploymentConsoleDialog
from .deployment_runner import DeploymentRunner, RedetectRunner, RollbackRunner
from .key_deploy_runner import KeyDeployRunner
from .maintenance_dialog import MaintenanceDialog, RollbackConfirmBox
from .server_edit_dialog import ServerEditDialog

if TYPE_CHECKING:
    from src.ui.window.main_window import MainWindow


# 部署状态展示元数据: (文案, 语义等级 - 用于 InfoBadge / QSS 选择器)
# 等级走 Fluent 语义色, 颜色由主题决定, 不再硬编码 hex.
_DEPLOYMENT_META: dict[DeploymentState, tuple[str, str]] = {
    DeploymentState.UNDEPLOYED: ("未部署", "info"),
    DeploymentState.DEPLOYING: ("部署中", "attention"),
    DeploymentState.DEPLOYED: ("已部署", "success"),
    DeploymentState.FAILED: ("部署失败", "error"),
}


def _make_state_badge(state: DeploymentState, parent: QWidget | None = None) -> InfoBadge:
    """根据部署状态创建 Fluent 语义化徽章. 

    颜色由 qfluentwidgets 主题色板决定, 自动适配深/浅色主题. 
    """
    text, level = _DEPLOYMENT_META[state]
    if level == "success":
        return InfoBadge.success(text, parent=parent)
    if level == "error":
        return InfoBadge.error(text, parent=parent)
    if level == "attention":
        return InfoBadge.attension(text, parent=parent)  # qfluentwidgets API 拼写
    return InfoBadge.info(text, parent=parent)


class RemoteUsageNoticeBox(MessageBoxBase):
    def __init__(self, parent: QWidget) -> None:
        super().__init__(parent=parent)

        self.title_label = TitleLabel(self.tr("远程功能使用前提示"), self)
        self.content_label = BodyLabel(
            self.tr(
                "远程服务器功能现已支持 Debian 系（Debian / Ubuntu）与 RHEL 系"
                "（CentOS / Rocky Linux / AlmaLinux / Fedora），CPU 架构覆盖 amd64 与 arm64。\n\n"
                "项目主要在 Ubuntu 24 上完成了完整实测，其他发行版以分发逻辑覆盖为主；"
                "首次部署时将自动跑一次远端兼容性体检，体检结果会在部署日志里以 [PREFLIGHT] 行展示，"
                "若提示「未识别的发行版但探测到可用包管理器」属正常情况，会以通用流程尝试部署。\n\n"
                "若你的发行版不在上面列表内（例如 Arch Linux / openSUSE / Alpine），"
                "暂不在本期支持边界内，建议提交 Issue 反馈实际诉求。\n\n"
                "此功能会连接你的服务器并执行安装、更新、回滚等远端操作，存在一定危险性。"
                "项目已经尽可能完善校验与保护，但仍请你根据自身情况谨慎使用。"
            ),
            self,
        )
        self.content_label.setWordWrap(True)
        self.issue_label = CaptionLabel(
            self.tr("如果使用过程中遇到问题，请提交 Issue 并提供必要信息，方便定位和修复。"),
            self,
        )
        self.issue_label.setWordWrap(True)
        self.accept_checkbox = CheckBox(
            self.tr("我已阅读并愿意使用；遇到问题会通过 Issue 理性反馈并协助定位。"),
            self,
        )

        self.widget.setMinimumSize(560, 340)
        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addWidget(self.content_label)
        self.viewLayout.addWidget(self.issue_label)
        self.viewLayout.addWidget(self.accept_checkbox)

        self.yesButton.setText(self.tr("确认并启用"))
        self.cancelButton.setText(self.tr("暂不使用"))
        self.yesButton.setEnabled(False)
        self.cancelButton.setDefault(True)
        self.yesButton.setDefault(False)
        self.accept_checkbox.toggled.connect(self.yesButton.setEnabled)

    def is_accepted(self) -> bool:
        return self.accept_checkbox.isChecked()


# ============================================================
# 服务器卡片 (自包含: 表单信息 + 头部动作 + 部署进度)
# ============================================================
class ServerCard(HeaderCardWidget):
    """单台服务器的自包含管理卡片 (Fluent HeaderCardWidget).

    布局原则 (按用户反馈对齐 v3):
    - 所有动作按钮放置在 Header 顶栏, Body 区不含交互控件
    - Body 用 QFormLayout 表单展示结构化信息 (主机 / 登录 / NapCat / LinuxQQ)
    - 部署按钮在 DEPLOYED 状态隐藏 (context-aware)
    - 维护操作 (刷新版本 / 强制更新 / 强制重装 / 回滚) 全部收纳在 [`MaintenanceDialog`]
    - 部署进度条仅部署 / 强制更新 / 回滚 时可见

    Header 布局: [title] [stretch] [InfoBadge] [test] [edit] [delete] [deploy] [maintenance]

    所有动作通过 Qt 信号上抛给 [`RemotePage`], 卡片本身不做业务决策. 
    """

    deploy_requested = Signal(str)        # server_id
    test_requested = Signal(str)
    edit_requested = Signal(str)
    delete_requested = Signal(str)
    maintenance_requested = Signal(str)   # 打开维护对话框

    def __init__(self, profile: ServerProfile, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._profile = profile
        self._state_badge: InfoBadge | None = None
        self.setObjectName("serverCard")
        self.setTitle(profile.name)
        self._build_layout()

    @property
    def profile(self) -> ServerProfile:
        return self._profile

    @property
    def server_id(self) -> str:
        return self._profile.id

    # ---------- 构建 ----------
    def _build_layout(self) -> None:
        """卡片内部布局.

        Header (从左到右):
            title | stretch | InfoBadge(状态) | test | edit | delete | deploy | maintenance

        Body (QFormLayout 表单):
            主机:    {host}:{port}
            登录:    {user} (使用{私钥|密码})
            NapCat:  {version|未探测}
            LinuxQQ: {version|未探测}

        Body 下方: ProgressBar + CaptionLabel (默认隐藏)
        """
        self.setFixedWidth(440)

        # ---------------- Header 区: 状态徽章 + 动作按钮组 ----------------
        self.headerLayout.addStretch()

        self._state_badge = _make_state_badge(self._profile.deployment_state, parent=self)
        self.headerLayout.addWidget(self._state_badge, 0, Qt.AlignmentFlag.AlignVCenter)
        self.headerLayout.addSpacing(8)

        # 通用工具按钮 (始终可见)
        self.test_btn = TransparentToolButton(FI.GLOBE, self)
        self.edit_btn = TransparentToolButton(FI.EDIT, self)
        self.delete_btn = TransparentToolButton(FI.DELETE, self)
        # 状态相关按钮 (动态显示/隐藏 by update_button_state)
        self.deploy_btn = TransparentToolButton(FI.SEND, self)
        self.maintenance_btn = TransparentToolButton(FI.SETTING, self)

        # W10b: 根据 backend_flavor 动态 deploy 按钮 tooltip
        is_sl = self._profile.backend_flavor == BackendFlavor.SNOWLUMA
        deploy_tip = self.tr("部署 SnowLuma") if is_sl else self.tr("部署 NapCat")

        self.test_btn.setToolTip(self.tr("测试 SSH 连接"))
        self.edit_btn.setToolTip(self.tr("编辑服务器配置"))
        self.delete_btn.setToolTip(self.tr("删除服务器"))
        self.deploy_btn.setToolTip(deploy_tip)
        self.maintenance_btn.setToolTip(self.tr("维护 (刷新版本 / 强制更新 / 回滚)"))

        for btn in (
            self.test_btn,
            self.edit_btn,
            self.delete_btn,
            self.deploy_btn,
            self.maintenance_btn,
        ):
            btn.setFixedSize(30, 30)
            btn.setToolTipDuration(1500)
            btn.installEventFilter(ToolTipFilter(btn, showDelay=300))
            self.headerLayout.addWidget(btn, 0, Qt.AlignmentFlag.AlignVCenter)

        # ---------------- Body 区: QFormLayout 结构化信息 ----------------
        body_widget = QWidget(self)
        body_widget.setObjectName("serverCardBody")

        form = QFormLayout(body_widget)
        form.setContentsMargins(20, 4, 20, 12)
        form.setHorizontalSpacing(16)
        form.setVerticalSpacing(8)
        form.setLabelAlignment(Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter)

        cred = self._profile.credentials
        auth_text = self.tr("私钥") if cred.auth_method == "key" else self.tr("密码")

        self.host_value = BodyLabel(f"{cred.host}:{cred.port}", self)
        self.host_value.setObjectName("serverCardValue")
        self.login_value = BodyLabel(f"{cred.username} ({auth_text})", self)
        self.login_value.setObjectName("serverCardValue")

        # W10b: flavor 分支
        # - NC: "NapCat {version}" + "LinuxQQ {version}"
        # - SL: "SnowLuma {framework_version}" + "LinuxQQ {version}"
        # 复用 napcat_value 跟 label 字段 (不再动态改 label 以简化 update_profile);
        # SL 的整体版本语义存放到 napcat_value 传递器中
        if is_sl:
            primary_label_text = "SnowLuma"
            primary_version = self._profile.snowluma_framework_version
        else:
            primary_label_text = "NapCat"
            primary_version = self._profile.napcat_version
        self.napcat_value = BodyLabel(self._format_version(primary_version), self)
        self.napcat_value.setObjectName("serverCardValue")
        self.qq_value = BodyLabel(self._format_version(self._profile.qq_version), self)
        self.qq_value.setObjectName("serverCardValue")

        form.addRow(self._make_label("主机"), self.host_value)
        form.addRow(self._make_label("登录"), self.login_value)
        form.addRow(self._make_label(primary_label_text), self.napcat_value)
        form.addRow(self._make_label("LinuxQQ"), self.qq_value)

        self.viewLayout.addWidget(body_widget, 1)

        # ---------------- 部署进度区 (默认隐藏) ----------------
        self.progress_widget = QWidget(self)
        self.progress_widget.setObjectName("serverCardProgress")
        prog_lay = QVBoxLayout(self.progress_widget)
        prog_lay.setContentsMargins(20, 0, 20, 12)
        prog_lay.setSpacing(4)
        self.progress_bar = ProgressBar(self.progress_widget)
        self.progress_bar.setRange(0, 100)
        self.progress_bar.setValue(0)
        self.progress_message = CaptionLabel("", self.progress_widget)
        self.progress_message.setObjectName("serverCardProgressMessage")
        self.progress_message.setWordWrap(True)
        prog_lay.addWidget(self.progress_bar)
        prog_lay.addWidget(self.progress_message)
        self.viewLayout.addWidget(self.progress_widget)
        self.progress_widget.hide()

        # ---------------- 信号绑定 ----------------
        sid = self._profile.id
        self.deploy_btn.clicked.connect(lambda: self.deploy_requested.emit(sid))
        self.test_btn.clicked.connect(lambda: self.test_requested.emit(sid))
        self.edit_btn.clicked.connect(lambda: self.edit_requested.emit(sid))
        self.delete_btn.clicked.connect(lambda: self.delete_requested.emit(sid))
        self.maintenance_btn.clicked.connect(lambda: self.maintenance_requested.emit(sid))

    @staticmethod
    def _make_label(text: str) -> StrongBodyLabel:
        """表单左侧字段名 (粗体, 较暗色)."""
        label = StrongBodyLabel(f"{text}")
        label.setObjectName("serverCardFieldLabel")
        return label

    @staticmethod
    def _format_version(version: str | None) -> str:
        return version if version else "未探测"

    # ---------- 公共 API: 由 RemotePage 调用 ----------
    def update_profile(self, profile: ServerProfile) -> None:
        """根据新 profile 刷新所有表单字段 (server_id 必须保持一致). """
        self._profile = profile
        self.setTitle(profile.name)
        cred = profile.credentials
        auth_text = self.tr("私钥") if cred.auth_method == "key" else self.tr("密码")
        self.host_value.setText(f"{cred.host}:{cred.port}")
        self.login_value.setText(f"{cred.username} ({auth_text})")
        # W10b: flavor 分支; D8 决策下 flavor 不变, label 无需刷新, 仅更新版本值
        if profile.backend_flavor == BackendFlavor.SNOWLUMA:
            self.napcat_value.setText(self._format_version(profile.snowluma_framework_version))
        else:
            self.napcat_value.setText(self._format_version(profile.napcat_version))
        self.qq_value.setText(self._format_version(profile.qq_version))
        # 状态徽章重建 (InfoBadge level 不可热切换)
        if self._state_badge is not None:
            self.headerLayout.removeWidget(self._state_badge)
            self._state_badge.setParent(None)
            self._state_badge.deleteLater()
        self._state_badge = _make_state_badge(profile.deployment_state, parent=self)
        # 重建后插回到 stretch 之后, 工具按钮之前
        # headerLayout 结构: [title] [stretch] [badge] [spacer] [tools...]
        # removeWidget 不会重排 layout 中其他 item 的索引, 直接 insertWidget 在 stretch 之后即可
        # 简化处理: 取 stretch 后第一个非 button 位置, 直接 addWidget 到末尾会破坏顺序
        # 故采用 insertWidget(1) - index 0 是 title (由 HeaderCardWidget 管理),
        # index 1 是 stretch, 我们插在 stretch 之后 (index 2 即 stretch 后)
        self.headerLayout.insertWidget(2, self._state_badge, 0, Qt.AlignmentFlag.AlignVCenter)

    def update_button_state(self, *, is_deploying_self: bool) -> None:
        """根据当前部署状态更新各按钮的 visible / enabled 标志.

        规则:
          - 部署进行时所有交互按钮禁用 (避免叠加触发)
          - deploy_btn:      DEPLOYED 隐藏; UNDEPLOYED / FAILED / DEPLOYING 显示
          - maintenance_btn: 始终显示, 仅 DEPLOYED / FAILED 时可点 (FAILED 用于回滚)
        """
        state = self._profile.deployment_state
        not_busy = not is_deploying_self

        # 通用按钮: 任何非 busy 状态都可用
        self.test_btn.setEnabled(not_busy)
        self.edit_btn.setEnabled(not_busy)
        self.delete_btn.setEnabled(not_busy)

        # 部署: DEPLOYED 隐藏 (没必要再显示)
        self.deploy_btn.setVisible(state is not DeploymentState.DEPLOYED)
        self.deploy_btn.setEnabled(not_busy)

        # 维护: 始终可见, 但仅 DEPLOYED + FAILED 可用
        # (FAILED 状态下用户仍可通过维护 -> 回滚 来清理失败残留)
        self.maintenance_btn.setEnabled(
            not_busy and state in (DeploymentState.DEPLOYED, DeploymentState.FAILED)
        )

    # ---------- 部署进度 ----------
    def show_progress(self, message: str, percent: int) -> None:
        self.progress_widget.show()
        self.progress_bar.setValue(max(0, min(100, percent)))
        self.progress_message.setText(f"{percent}% — {message}" if message else f"{percent}%")

    def hide_progress(self) -> None:
        self.progress_widget.hide()
        self.progress_bar.setValue(0)
        self.progress_message.setText("")


# ============================================================
# 主页面 - 单列卡片流
# ============================================================
class RemotePage(QWidget):
    """远程服务器管理页面. 

    架构: 单列垂直滚动的 [`ServerCard`] 卡片流, 每张卡片自包含全部信息和操作. 
    去掉了 master-detail 双栏布局, 也无 "选中" 视觉概念. 

    历史兼容: 为保持 [`script.test.test_remote_page_actions`] 测试稳定,
    本页保留 ``self._active_server_id`` 与 [`select_server`] 方法. 卡片信号触发
    `_on_xxx` 时会自动设置 ``_active_server_id``; 测试 / 程序内部以无参方式调用
    `_on_xxx` 时会回退到该字段. 
    """

    def __init__(self) -> None:
        super().__init__()
        self._cards: dict[str, ServerCard] = {}
        # P1.5: server_id -> 已打开的部署控制台 (避免对同一台服务器重复弹窗)
        self._consoles: dict[str, DeploymentConsoleDialog] = {}
        # 兼容旧测试: 卡片信号触发 / select_server 时设置, 无参 _on_xxx 回退至此
        self._active_server_id: str | None = None
        self._usage_notice_prompting = False

    def initialize(self, parent: "MainWindow") -> "RemotePage":
        """页面初始化, 由主窗口在创建时调用. """
        self.setParent(parent)
        self.setObjectName("RemotePage")

        self._build_ui()
        self._connect_manager_signals()
        self._reload()

        PageStyleSheet.REMOTE.apply(self)
        return self

    def showEvent(self, event: QShowEvent) -> None:  # noqa: N802 - Qt 重写
        super().showEvent(event)
        if not cfg.get(cfg.remote_usage_notice_accepted) and not self._usage_notice_prompting:
            QTimer.singleShot(0, self._prompt_usage_notice_if_needed)

    def _prompt_usage_notice_if_needed(self) -> None:
        if self.isVisible() and not cfg.get(cfg.remote_usage_notice_accepted):
            self._ensure_usage_notice_accepted(notify_cancel=False)

    def _ensure_usage_notice_accepted(self, *, notify_cancel: bool = True) -> bool:
        if cfg.get(cfg.remote_usage_notice_accepted):
            return True
        if self._usage_notice_prompting:
            return False

        self._usage_notice_prompting = True
        try:
            dialog = RemoteUsageNoticeBox(self.window())
            if dialog.exec() and dialog.is_accepted():
                cfg.set(cfg.remote_usage_notice_accepted, True)
                success_bar(self.tr("已启用远程功能"), parent=self)
                return True
        finally:
            self._usage_notice_prompting = False

        if notify_cancel:
            info_bar(self.tr("未确认远程功能使用提示，本次操作已取消"), parent=self)
        return False

    # ---------- UI 构建 ----------
    def _build_ui(self) -> None:
        root = QVBoxLayout(self)
        root.setContentsMargins(24, 20, 24, 20)
        root.setSpacing(14)

        self.title_label = TitleLabel("远程服务器管理", self)
        self.subtitle_label = CaptionLabel(
            "添加 Linux 服务器, 即可在远端运行 Bot, 操作体验与本地一致",
            self,
        )
        root.addWidget(self.title_label)
        root.addWidget(self.subtitle_label)

        # ---- 多列卡片流 (FlowLayout) ----
        self._scroll = ScrollArea(self)
        self._scroll.setObjectName("remoteScrollArea")
        self._scroll.setWidgetResizable(True)
        self._scroll.setFrameShape(ScrollArea.Shape.NoFrame)

        self._list_inner = QWidget(self._scroll)
        self._list_inner.setObjectName("listInnerWidget")
        self._list_layout = FlowLayout(self._list_inner)
        self._list_layout.setContentsMargins(0, 0, 4, 0)
        self._list_layout.setSpacing(12)

        self._scroll.setWidget(self._list_inner)
        root.addWidget(self._scroll, 1)

        # 空态文案
        self._empty_label = BodyLabel(
            "尚未添加任何服务器, 点击右下角 + 按钮添加。", self
        )
        self._empty_label.setObjectName("listEmptyLabel")
        self._empty_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._list_layout.addWidget(self._empty_label)

        # ---- 浮动操作按钮 (FAB), 与 BotListPage 视觉对齐 ----
        # 父级直接挂在 RemotePage 上, 由 resizeEvent 浮动定位到右下角;
        # 不进入任何 layout, 始终覆盖在 ScrollArea 之上.
        self.add_btn = PrimaryToolButton(FI.ADD, self)
        self.refresh_btn = ToolButton(FI.UPDATE, self)
        # P4 W2 F4: 状态总览入口 - 第三个浮动按钮, 位置在 refresh 之上
        self.overview_btn = ToolButton(FI.PIE_SINGLE, self)
        for btn, tip in (
            (self.add_btn, "添加服务器"),
            (self.refresh_btn, "刷新"),
            (self.overview_btn, "状态总览"),
        ):
            btn.setFixedSize(40, 40)
            btn.setIconSize(QSize(20, 20))
            btn.setToolTip(tip)
            btn.setToolTipDuration(1500)
            btn.installEventFilter(ToolTipFilter(btn, showDelay=300))
        # 保证浮在卡片之上 (Qt 同父子级按添加顺序绘制, 后插入者更高)
        self.add_btn.raise_()
        self.refresh_btn.raise_()
        self.overview_btn.raise_()

        # ---- 信号 ----
        self.add_btn.clicked.connect(self._on_add)
        self.refresh_btn.clicked.connect(self._on_refresh)
        self.overview_btn.clicked.connect(self._on_open_overview)

        self._refresh_card_states()

    # ---------- 重写方法 ----------
    def resizeEvent(self, event: QResizeEvent) -> None:  # noqa: N802 - Qt 重写
        """页面尺寸变化时把 FAB 钉在 ScrollArea 区域的右下角.

        视觉对齐目标: [`BotListPage.resizeEvent`](src/ui/page/bot_page/sub_page/bot_list.py).
        ``BotListPage`` 本身继承 ``ScrollArea``, 它的 ``self.width()/height()`` 直接
        就是滚动区域大小; 而 ``RemotePage`` 是 QWidget, 上方还有 title / subtitle,
        因此我们改用 ``self._scroll`` 的几何信息来计算锚点, 避免 FAB 跟着标题区
        位移. 偏移量(-16 / -32 / -82)与 BotListPage 完全相同.
        """
        super().resizeEvent(event)  # 触发 Qt 布局, 此后 self._scroll 尺寸即为新值
        if not hasattr(self, "add_btn") or not hasattr(self, "_scroll"):
            return

        # ScrollArea 在 RemotePage 坐标系下的位置 + 大小
        scroll_geom = self._scroll.geometry()
        x = scroll_geom.x() + scroll_geom.width() - self.add_btn.width() - 16
        y_base = scroll_geom.y() + scroll_geom.height() - self.add_btn.height()
        self.add_btn.move(x, y_base - 32)
        self.refresh_btn.move(x, y_base - 82)
        if hasattr(self, "overview_btn"):
            self.overview_btn.move(x, y_base - 132)

    def _connect_manager_signals(self) -> None:
        manager = it(ServerManager)
        manager.server_added.connect(lambda *_: self._reload())
        manager.server_updated.connect(lambda *_: self._reload())
        manager.server_removed.connect(lambda *_: self._reload())
        manager.server_state_changed.connect(lambda *_: self._reload())
        # P1: 部署进度 / 完成
        manager.deployment_progress.connect(self._on_deployment_progress)
        manager.deployment_finished.connect(self._on_deployment_finished)

    # ---------- 列表渲染 ----------
    def _reload(self) -> None:
        manager = it(ServerManager)
        servers = manager.list_servers()

        # 简单粗暴: 全删全建 (服务器一般 1-3 台, 重建成本可忽略)
        for card in self._cards.values():
            self._list_layout.removeWidget(card)
            card.deleteLater()
        self._cards.clear()

        if not servers:
            self._empty_label.show()
            self._refresh_card_states()
            return

        self._empty_label.hide()
        for index, profile in enumerate(servers):
            card = self._create_card(profile)
            self._cards[profile.id] = card
            self._list_layout.insertWidget(index, card)

        self._refresh_card_states()

    def _create_card(self, profile: ServerProfile) -> ServerCard:
        """创建单张卡片并把卡片信号路由到 RemotePage 的 _on_xxx 槽. """
        card = ServerCard(profile, parent=self._list_inner)
        card.deploy_requested.connect(self._on_deploy)
        card.test_requested.connect(self._on_test)
        card.edit_requested.connect(self._on_edit)
        card.delete_requested.connect(self._on_delete)
        card.maintenance_requested.connect(self._on_open_maintenance)
        return card

    def _refresh_card_states(self) -> None:
        """根据当前 ServerManager 状态刷新所有卡片的按钮 enabled 标志. """
        manager = it(ServerManager)
        any_deploying = bool(getattr(manager, "_deploying", set()))
        # 部署进行时禁用 "添加", 防止字典迭代受扰
        self.add_btn.setEnabled(not any_deploying)
        for sid, card in self._cards.items():
            card.update_button_state(is_deploying_self=manager.is_deploying(sid))

    # ---------- 兼容旧 API: 测试用 ----------
    def select_server(self, server_id: str) -> None:
        """[兼容旧测试] 把 server_id 设为 "活跃" 卡片. 

        新交互模型下用户直接点击各卡片的按钮; 本方法仅供测试 / 程序内部驱动. 
        """
        if server_id in self._cards:
            self._active_server_id = server_id
        self._refresh_card_states()

    # ---------- 内部: 解析 server_id ----------
    def _resolve_sid(self, server_id: str | None) -> str | None:
        """优先使用显式参数, 否则回退到 ``_active_server_id``; 同时记忆为 active."""
        sid = server_id or self._active_server_id
        if sid and sid in self._cards:
            self._active_server_id = sid
            return sid
        return None

    # ============================================================
    # 操作回调 - 由卡片信号触发或测试代码直接调用
    # ============================================================
    def _on_add(self) -> None:
        if not self._ensure_usage_notice_accepted():
            return
        manager = it(ServerManager)
        # P4 F5.2: 让对话框使用与 ServerManager 同款的 CredentialStore, 复用其
        # ``is_available`` 探测结果, 避免实例间结论不一致.
        dialog = ServerEditDialog(
            self.window(),
            credential_store=manager.credential_store(),
        )
        if dialog.exec():
            try:
                profile = dialog.get_profile()
                password = dialog.get_password()
            except ValueError as exc:
                error_bar(str(exc), parent=self)
                return
            manager.add_server(
                profile,
                password=password,
                remember_password=dialog.wants_remember_password(),
            )
            self._active_server_id = profile.id
            success_bar(f"已添加服务器: {profile.name}", parent=self)
            # 勾选了"自动配置 SSH 密钥": 走后台 runner 推送公钥并切到密钥认证.
            # 失败仅 InfoBar 提示, 档案保持密码认证不变, 用户可重试.
            if dialog.wants_auto_setup_key() and password:
                self._dispatch_key_deploy(profile.id, password)

    def _on_edit(self, server_id: str | None = None) -> None:
        if not self._ensure_usage_notice_accepted():
            return
        sid = self._resolve_sid(server_id)
        if not sid:
            return
        manager = it(ServerManager)
        profile = manager.get_server(sid)
        if profile is None:
            return
        existing_password = manager._password_cache.get(profile.id)  # noqa: SLF001
        # P4 F5.2: 把 keyring 当前是否记忆了该服务器的密码透传给对话框默认勾选项.
        dialog = ServerEditDialog(
            self.window(),
            profile=profile,
            existing_password=existing_password,
            credential_store=manager.credential_store(),
            existing_remember_password=manager.has_remembered_password(profile.id),
        )
        if dialog.exec():
            try:
                updated = dialog.get_profile()
                password = dialog.get_password()
            except ValueError as exc:
                error_bar(str(exc), parent=self)
                return
            # 用户取消勾选 -> 显式 False (从 keyring 删除); 仍勾选 -> True (写入)
            manager.update_server(
                updated,
                password=password,
                remember_password=dialog.wants_remember_password(),
            )
            success_bar(f"已更新服务器: {updated.name}", parent=self)
            # 编辑模式同样支持"自动配置 SSH 密钥"勾选.
            if dialog.wants_auto_setup_key() and password:
                self._dispatch_key_deploy(updated.id, password)

    # ---------- 自动配置 SSH 密钥 (ssh-copy-id 等价物) ----------
    def _dispatch_key_deploy(self, server_id: str, password: str) -> None:
        """提交 [`KeyDeployRunner`] 到 ssh 池, 完成回调走 InfoBar.

        密码仅作为运行器参数透传给后台线程, 不会写盘也不会进入任何全局状态.
        """
        runner = KeyDeployRunner(server_id, password=password)
        runner.signals.finished.connect(self._on_key_deploy_finished)
        remote_ssh_pool().start(runner)

    def _on_key_deploy_finished(self, server_id: str, ok: bool, message: str) -> None:  # noqa: ARG002
        # 成败提示统一走 InfoBar; 进度悬浮提示由 ProgressInfoBar 桥负责.
        # 卡片刷新通过 ServerManager.server_updated 信号自动触发, 这里无需手动 reload.
        if ok:
            success_bar(message or "已配置免密登录", parent=self)
        else:
            error_bar(message or "SSH 密钥配置失败", parent=self)

    def _on_test(self, server_id: str | None = None) -> None:
        if not self._ensure_usage_notice_accepted():
            return
        sid = self._resolve_sid(server_id)
        if not sid:
            return
        manager = it(ServerManager)
        profile = manager.get_server(sid)
        if profile is None:
            return
        password = manager._password_cache.get(profile.id) if profile.credentials.auth_method == "password" else None  # noqa: SLF001
        if profile.credentials.auth_method == "password" and not password:
            error_bar("密码认证模式下未保存密码, 请先编辑并填写密码", parent=self)
            return

        # P3 perf: 进行中 / 完成状态由
        # [`ProgressInfoBarBridge`](src/ui/components/progress_info_bar_bridge.py)
        # 统一在右上 ProgressInfoBar 展示, 这里不再额外 info_bar.
        # 仅禁用本卡片的测试按钮, 防止重复点击
        if sid in self._cards:
            self._cards[sid].test_btn.setEnabled(False)

        tester = ConnectionTester(profile, password=password)
        tester.signals.finished.connect(self._on_test_finished)
        remote_ssh_pool().start(tester)

    def _on_test_finished(self, server_id: str, ok: bool, message: str) -> None:  # noqa: ARG002
        # P3 perf: 成败提示已走 ProgressInfoBar 桥, 这里仅负责
        # 更新 last_connected_at 与恢复按钮可用性.
        self._refresh_card_states()
        manager = it(ServerManager)
        profile = manager.get_server(server_id)
        if ok and profile is not None:
            from time import time

            profile.last_connected_at = time()
            manager.update_server(profile)
        # 恢复本卡片测试按钮
        if server_id in self._cards:
            self._cards[server_id].test_btn.setEnabled(True)

    def _on_delete(self, server_id: str | None = None) -> None:
        sid = self._resolve_sid(server_id)
        if not sid:
            return
        manager = it(ServerManager)
        profile = manager.get_server(sid)
        if profile is None:
            return

        # W10b: 按 backend_flavor 切换删除确认文案
        if profile.backend_flavor == BackendFlavor.SNOWLUMA:
            delete_hint = "该操作不会影响远端已部署的 SnowLuma。"
        else:
            delete_hint = "该操作不会影响远端已部署的 NapCat。"
        ask = AskBox(
            "确认删除",
            f"确定要删除服务器 “{profile.name}” 吗？\n{delete_hint}",
            self.window(),
        )
        if ask.exec():
            removed = manager.remove_server(profile.id)
            if removed:
                if self._active_server_id == profile.id:
                    self._active_server_id = None
                info_bar(f"已删除服务器: {profile.name}", parent=self)

    # ---------- P1: 部署 ----------
    def _on_deploy(self, server_id: str | None = None) -> None:
        if not self._ensure_usage_notice_accepted():
            return
        sid = self._resolve_sid(server_id)
        if not sid:
            return
        manager = it(ServerManager)
        profile = manager.get_server(sid)
        if profile is None:
            return

        if manager.is_deploying(profile.id):
            # 保留: 并非任务进行中反馈, 而是"已有任务占用"的拦截提示, 不走桥
            info_bar(f"[{profile.name}] 正在部署中, 请耐心等待", parent=self)
            return

        # 密码认证模式必须先有缓存密码
        if profile.credentials.auth_method == "password":
            password = manager._password_cache.get(profile.id)  # noqa: SLF001
            if not password:
                error_bar("密码认证模式下未保存密码, 请先编辑并填写密码", parent=self)
                return

        # 已部署时二次确认 (W10b: flavor 分支文案)
        if profile.deployment_state == DeploymentState.DEPLOYED:
            if profile.backend_flavor == BackendFlavor.SNOWLUMA:
                redeploy_detail = (
                    "重新部署会重新执行 LinuxQQ 与 SnowLuma.Framework 的安装脚本"
                    "（脚本会自动跳过已存在的组件）。"
                )
            else:
                redeploy_detail = (
                    "重新部署会重新执行 LinuxQQ 与 NapCat 的安装脚本"
                    "（脚本会自动跳过已存在的组件）。"
                )
            ask = AskBox(
                "确认重新部署",
                f"服务器 “{profile.name}” 已处于已部署状态。\n\n"
                f"{redeploy_detail}\n"
                "是否继续？",
                self.window(),
            )
            if not ask.exec():
                return

        # P3 perf: "开始部署" 进度交由 ProgressInfoBar 桥; 卡片内部进度条依旧保留
        if sid in self._cards:
            self._cards[sid].show_progress("准备部署", 0)
        self._refresh_card_states()

        # P1.5: 弹出独立的部署控制台
        self._open_or_focus_console(profile.id, profile.name)

        runner = DeploymentRunner(profile.id)
        runner.signals.finished.connect(self._on_deployment_runner_finished)
        remote_ssh_pool().start(runner)

    def _open_or_focus_console(self, server_id: str, server_name: str) -> None:
        """打开或前置 DeploymentConsoleDialog. """
        existing = self._consoles.get(server_id)
        if existing is not None and existing.isVisible():
            existing.raise_()
            existing.activateWindow()
            return

        console = DeploymentConsoleDialog(server_id, server_name, parent=self.window())
        console.destroyed.connect(lambda *_args, sid=server_id: self._consoles.pop(sid, None))
        self._consoles[server_id] = console
        console.show()
        console.raise_()
        console.activateWindow()

    def _on_deployment_progress(self, server_id: str, message: str, percent: int) -> None:
        # 直接定位到对应卡片展示进度 (不再受 "选中" 限制)
        if server_id in self._cards:
            self._cards[server_id].show_progress(message, percent)

    def _on_deployment_finished(self, server_id: str, ok: bool, message: str) -> None:  # noqa: ARG002
        # P3 perf: 成败 / 失败提示由 ProgressInfoBar 桥统一展示;
        # 这里仅隐藏卡片内部进度条 + 刷新按钮状态.
        if server_id in self._cards:
            self._cards[server_id].hide_progress()
        self._refresh_card_states()

    def _on_deployment_runner_finished(self, server_id: str) -> None:  # noqa: ARG002
        # runner 收尾(已经由 deployment_finished 处理 UI), 这里仅做按钮状态保险刷新
        self._refresh_card_states()

    # ---------- P3.W2 (A): 单台刷新版本 / 强制更新 / 强制重装 ----------
    def _on_redetect_versions_selected(self, server_id: str | None = None) -> None:
        """对指定 (或当前活跃) 服务器后台探测版本, 不重跑安装脚本. """
        if not self._ensure_usage_notice_accepted():
            return
        sid = self._resolve_sid(server_id)
        if not sid:
            return
        manager = it(ServerManager)
        profile = manager.get_server(sid)
        if profile is None:
            return
        if manager.is_deploying(profile.id):
            info_bar(f"[{profile.name}] 正在部署中, 请稍后重试", parent=self)
            return
        # P3 perf: 进行中 / 完成反馈已交给 ProgressInfoBar 桥
        runner = RedetectRunner(profile.id)
        runner.signals.finished.connect(self._on_redetect_finished)
        remote_ssh_pool().start(runner)

    def _on_force_update_napcat(self, server_id: str | None = None) -> None:
        """强制重跑 install_napcat (NC) / 重新部署 SnowLuma.Framework (SL).

        W10b-Maintenance: 按 backend_flavor 切换文案 + 旗标:

        - NC: ``force_napcat_update=True`` (传给 ``LinuxCoreDeployment.install_napcat``)
        - SL: ``force_snowluma_redeploy=True`` 走 ``_deploy_snowluma_flavor``
          的 SL framework + launcher 重新上传路径 (force_napcat_update 在 SL flavor
          下被 ``ServerManager.deploy_server`` 忽略, 不会误触 NC 路径)
        """
        sid = self._resolve_sid(server_id)
        if not sid:
            return
        manager = it(ServerManager)
        profile = manager.get_server(sid)
        if profile is None:
            return

        is_sl = profile.backend_flavor == BackendFlavor.SNOWLUMA
        if is_sl:
            label = "重新部署 SnowLuma.Framework"
            ask_message = (
                "即将重新上传 Desktop 内置的 SnowLuma.Framework lite tarball,\n"
                "并重跑 install_snowluma 脚本 (同时重传 daemon / bot launcher 脚本)。\n\n"
                "期间该服务器上运行中的 SnowLuma Bot 会被中断, 完成后需手动重启。\n"
                "是否继续？"
            )
        else:
            label = "强制更新 NapCat"
            ask_message = (
                "即将强制重新下载并解压远端 NapCat 安装包。\n\n"
                "期间该服务器上运行中的 Bot 会被中断, 完成后需手动重启。\n是否继续？"
            )

        self._start_force_deploy(
            server_id=sid,
            label=label,
            ask_message=ask_message,
            force_napcat_update=True,
            force_linuxqq_reinstall=False,
        )

    def _on_force_reinstall_linuxqq(self, server_id: str | None = None) -> None:
        """强制重跑 install_linuxqq 并传 force_reinstall=True (比强制更新 NapCat / 重新部署 SL Framework 更重).

        W10b-Maintenance: 按 backend_flavor 切换文案. 流程逻辑两侧一致 (都是清掉
        ``${workspace}/opt/QQ/...`` 重新装 deb), 仅二次确认文案不同.
        """
        sid = self._resolve_sid(server_id)
        if not sid:
            return
        manager = it(ServerManager)
        profile = manager.get_server(sid)
        if profile is None:
            return

        is_sl = profile.backend_flavor == BackendFlavor.SNOWLUMA
        if is_sl:
            ask_message = (
                "即将强制重新下载并重装远端 LinuxQQ。\n\n"
                "该操作会备份 SnowLuma 配置 → 删除旧 LinuxQQ → 重新安装,\n"
                "耗时较长（依赖远端带宽）, 期间运行中的 SnowLuma Bot 会被中断。\n是否继续？"
            )
        else:
            ask_message = (
                "即将强制重新下载并重装远端 LinuxQQ。\n\n"
                "该操作会先备份 NapCat 配置 → 删除旧 LinuxQQ → 重新安装,\n"
                "耗时较长（依赖远端带宽）, 期间运行中的 Bot 会被中断。\n是否继续？"
            )

        self._start_force_deploy(
            server_id=sid,
            label="强制重装 LinuxQQ",
            ask_message=ask_message,
            force_napcat_update=False,
            force_linuxqq_reinstall=True,
        )

    def _start_force_deploy(
        self,
        *,
        server_id: str | None,
        label: str,
        ask_message: str,
        force_napcat_update: bool,
        force_linuxqq_reinstall: bool,
    ) -> None:
        """`强制更新/重装` 的公用路径: 二次确认 + 弹控制台 + 丢给 [`DeploymentRunner`]."""
        if not self._ensure_usage_notice_accepted():
            return
        sid = self._resolve_sid(server_id)
        if not sid:
            return
        manager = it(ServerManager)
        profile = manager.get_server(sid)
        if profile is None:
            return
        if manager.is_deploying(profile.id):
            info_bar(f"[{profile.name}] 正在部署中, 请稍后重试", parent=self)
            return
        if profile.credentials.auth_method == "password":
            password = manager._password_cache.get(profile.id)  # noqa: SLF001
            if not password:
                error_bar("密码认证模式下未保存密码, 请先编辑并填写密码", parent=self)
                return

        ask = AskBox(f"确认{label}", ask_message, self.window())
        if not ask.exec():
            return

        # P3 perf: {label}中... 进度交给 ProgressInfoBar 桥; 卡片内进度条依旧保留
        if sid in self._cards:
            self._cards[sid].show_progress(f"准备{label}", 0)
        self._refresh_card_states()
        self._open_or_focus_console(profile.id, profile.name)

        runner = DeploymentRunner(
            profile.id,
            force_napcat_update=force_napcat_update,
            force_linuxqq_reinstall=force_linuxqq_reinstall,
        )
        runner.signals.finished.connect(self._on_deployment_runner_finished)
        remote_ssh_pool().start(runner)

    # ---------- P3.W2: 维护对话框入口 ----------
    def _on_open_maintenance(self, server_id: str | None = None) -> None:
        """打开 [`MaintenanceDialog`] 让用户选择维护操作.

        所有维护操作 (刷新版本 / 强制更新 / 强制重装 / 回滚) 现在均通过此对话框访问,
        不再在卡片上散落多个按钮.
        """
        if not self._ensure_usage_notice_accepted():
            return
        sid = self._resolve_sid(server_id)
        if not sid:
            return
        manager = it(ServerManager)
        profile = manager.get_server(sid)
        if profile is None:
            return
        # 部署进行中拦截
        if manager.is_deploying(sid):
            error_bar(self.tr("该服务器正在部署中, 请稍后再试"), parent=self)
            return

        dialog = MaintenanceDialog(profile, parent=self.window())
        # 路由对话框选择 -> 既有 _on_xxx 方法 (内部仍走二次确认 + Runner)
        dialog.redetect_clicked.connect(lambda: self._on_redetect_versions_selected(sid))
        dialog.force_update_napcat_clicked.connect(lambda: self._on_force_update_napcat(sid))
        dialog.force_reinstall_linuxqq_clicked.connect(
            lambda: self._on_force_reinstall_linuxqq(sid)
        )
        dialog.rollback_clicked.connect(lambda: self._on_rollback(sid))
        dialog.exec()

    # ---------- P3.W2 (F): 回滚部署 ----------
    def _on_rollback(self, server_id: str | None = None) -> None:
        """手动触发远端部署回滚; 必须二次确认 + 选择 ``include_qq``."""
        if not self._ensure_usage_notice_accepted():
            return
        sid = self._resolve_sid(server_id)
        if not sid:
            return
        manager = it(ServerManager)
        profile = manager.get_server(sid)
        if profile is None:
            return
        if manager.is_deploying(profile.id):
            info_bar(f"[{profile.name}] 正在部署/回滚中, 请等待完成", parent=self)
            return
        if profile.credentials.auth_method == "password":
            password = manager._password_cache.get(profile.id)  # noqa: SLF001
            if not password:
                error_bar("密码认证模式下未保存密码, 请先编辑并填写密码", parent=self)
                return

        # W10b-Maintenance: flavor 分发文案 — SL 走 SnowLuma 语境
        is_sl = profile.backend_flavor == BackendFlavor.SNOWLUMA
        dialog = RollbackConfirmBox(profile.name, self.window(), is_snowluma=is_sl)
        if not dialog.exec():
            return
        include_qq = dialog.get_include_qq()

        # P3 perf: 回滚进度交给 ProgressInfoBar 桥
        if sid in self._cards:
            self._cards[sid].show_progress("准备回滚", 0)
        self._refresh_card_states()
        self._open_or_focus_console(profile.id, profile.name)

        runner = RollbackRunner(profile.id, include_qq=include_qq)
        runner.signals.finished.connect(self._on_rollback_finished)
        remote_ssh_pool().start(runner)

    def _on_rollback_finished(self, server_id: str, ok: bool, message: str) -> None:  # noqa: ARG002
        """[`RollbackRunner`] 完结回调; 业务消息已由 deployment_finished 走 info/error bar."""
        self._refresh_card_states()
        _ = ok, message  # noqa: F841

    # ---------- 刷新: 重载列表 + 后台批量探测版本 ----------
    def _on_refresh(self) -> None:
        """刷新按钮: 重载 UI + 对所有已部署的服务器后台触发版本探测. """
        if not self._ensure_usage_notice_accepted():
            return
        self._reload()
        manager = it(ServerManager)
        triggered: list[str] = []
        for profile in manager.list_servers():
            if profile.deployment_state is not DeploymentState.DEPLOYED:
                continue
            if manager.is_deploying(profile.id):
                continue
            runner = RedetectRunner(profile.id)
            runner.signals.finished.connect(self._on_redetect_finished)
            remote_ssh_pool().start(runner)
            triggered.append(profile.name)
        # P3 perf: 多台同时探测会 spawn 多个 ProgressInfoBar (堆叠于右上),
        # 不再额外的 info_bar 提示总计; 仅保留 ``triggered`` 变量供调试
        _ = triggered  # noqa: F841

    def _on_redetect_finished(
        self,
        server_id: str,  # noqa: ARG002 - 成败 / 失败提示走 ProgressInfoBar 桥
        ok: bool,
        napcat_version: object,
        qq_version: object,
        error_msg: str,
    ) -> None:
        # P3 perf: 成败 / 失败反馈由 RedetectRunner 进桥 → ProgressInfoBar 统一处理.
        # UI 处仅需记录变量以供调试 / 保留原有接口参数.
        _ = ok, napcat_version, qq_version, error_msg  # noqa: F841

    # ---------- P4 W2 F4: 状态总览入口 ----------
    def _on_open_overview(self) -> None:
        """打开 [`StatusOverviewDialog`](src/ui/components/status_overview_dialog.py).

        首次打开前确保
        [`ResourceMonitorService.bind_to_server_manager`](src/core/remote/resource_monitor.py)
        已绑定到 ``ServerManager``, 这样资源采样 worker 才会在用户真正需要总览时启动,
        与 W1/W2 既有路径解耦.
        """
        if not self._ensure_usage_notice_accepted():
            return
        # 项目内模块导入: 延迟 import 避免页面初始化阶段引入 W2 资源监控依赖
        from src.ui.components.status_overview_dialog import StatusOverviewDialog

        try:
            from src.core.remote.resource_monitor import ResourceMonitorService

            it(ResourceMonitorService).bind_to_server_manager()
        except Exception:  # noqa: BLE001 - bind 失败不应阻断对话框打开
            pass

        dialog = StatusOverviewDialog(parent=self.window())
        dialog.exec()


class RemotePageCreator(AbstractCreator, ABC):
    """远程页面创建器. """

    targets = (CreateTargetInfo("src.ui.page.remote_page", "RemotePage"),)

    @staticmethod
    def available() -> bool:
        return exists_module("src.ui.page.remote_page")

    def create(self, *args, **kwargs) -> RemotePage:
        return RemotePage(*args, **kwargs)


add_creator(RemotePageCreator)
