# -*- coding: utf-8 -*-
"""远程服务器管理页面（v2 架构）。

对应 [`docs/general/remote_ssh_plan.md`](../../../../../../docs/general/remote_ssh_plan.md) §3 与 §10。
v2 = VS Code Remote 模型: 本地 UI 透明代理远端 NapCat Core,
本页负责服务器档案的 CRUD 与连接测试; 部署 / Bot 绑定将在 P1 / P2 阶段加入。
"""

from __future__ import annotations

from abc import ABC
from datetime import datetime
from typing import TYPE_CHECKING

from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module, it
from PySide6.QtCore import Qt, QThreadPool
from PySide6.QtWidgets import (
    QFrame,
    QHBoxLayout,
    QScrollArea,
    QVBoxLayout,
    QWidget,
)
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    PrimaryPushButton,
    PushButton,
    StrongBodyLabel,
    SubtitleLabel,
    TitleLabel,
    FluentIcon as FI,
)

from src.desktop.core.remote import DeploymentState, ServerManager, ServerProfile
from src.desktop.ui.common.style_sheet import PageStyleSheet
from src.desktop.ui.components.info_bar import error_bar, info_bar, success_bar
from src.desktop.ui.components.message_box import AskBox

from .connection_tester import ConnectionTester
from .server_edit_dialog import ServerEditDialog

if TYPE_CHECKING:
    from src.desktop.ui.window.main_window import MainWindow


_DEPLOYMENT_LABEL = {
    DeploymentState.UNDEPLOYED: ("未部署", "#8a8a8a"),
    DeploymentState.DEPLOYING: ("部署中", "#0078d4"),
    DeploymentState.DEPLOYED: ("已部署", "#107c10"),
    DeploymentState.FAILED: ("部署失败", "#d83b01"),
}


# ============================================================
# 服务器卡片
# ============================================================
class ServerCard(QFrame):
    """单台服务器的列表项。"""

    def __init__(self, profile: ServerProfile, *, selected: bool = False, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._profile = profile
        self._selected = selected
        self.setObjectName("serverCard")
        self.setProperty("selected", selected)
        self.setMinimumHeight(72)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self._build_layout()
        self._apply_style()

    @property
    def profile(self) -> ServerProfile:
        return self._profile

    def set_selected(self, selected: bool) -> None:
        self._selected = selected
        self.setProperty("selected", selected)
        self._apply_style()

    def _build_layout(self) -> None:
        layout = QHBoxLayout(self)
        layout.setContentsMargins(14, 10, 14, 10)
        layout.setSpacing(12)

        info_layout = QVBoxLayout()
        info_layout.setSpacing(2)

        self.name_label = StrongBodyLabel(self._profile.name, self)
        info_layout.addWidget(self.name_label)

        cred = self._profile.credentials
        endpoint = f"{cred.username}@{cred.host}:{cred.port}"
        self.endpoint_label = CaptionLabel(endpoint, self)
        info_layout.addWidget(self.endpoint_label)

        layout.addLayout(info_layout, 1)

        state_text, state_color = _DEPLOYMENT_LABEL[self._profile.deployment_state]
        self.state_label = CaptionLabel(state_text, self)
        self.state_label.setStyleSheet(f"color: {state_color}; font-weight: 600;")
        layout.addWidget(self.state_label)

    def _apply_style(self) -> None:
        if self._selected:
            self.setStyleSheet(
                """
                #serverCard {
                    background: rgba(0, 120, 212, 0.12);
                    border: 1px solid rgba(0, 120, 212, 0.6);
                    border-radius: 8px;
                }
                """
            )
        else:
            self.setStyleSheet(
                """
                #serverCard {
                    background: rgba(128, 128, 128, 0.05);
                    border: 1px solid rgba(128, 128, 128, 0.15);
                    border-radius: 8px;
                }
                #serverCard:hover {
                    background: rgba(128, 128, 128, 0.10);
                }
                """
            )

    def mousePressEvent(self, event) -> None:  # noqa: N802 - Qt 重写
        if event.button() == Qt.MouseButton.LeftButton:
            page = self._find_page()
            if page is not None:
                page.select_server(self._profile.id)
        super().mousePressEvent(event)

    def _find_page(self) -> "RemotePage | None":
        widget: QWidget | None = self.parentWidget()
        while widget is not None:
            if isinstance(widget, RemotePage):
                return widget
            widget = widget.parentWidget()
        return None


# ============================================================
# 详情面板
# ============================================================
class ServerDetailPanel(QFrame):
    """选中服务器的详情展示。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setObjectName("detailPanel")
        self.setStyleSheet(
            """
            #detailPanel {
                background: rgba(128, 128, 128, 0.04);
                border: 1px solid rgba(128, 128, 128, 0.12);
                border-radius: 8px;
            }
            """
        )
        self._profile: ServerProfile | None = None
        self._build_layout()
        self.show_empty()

    def _build_layout(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(16, 14, 16, 14)
        layout.setSpacing(10)

        self.title_label = SubtitleLabel("服务器详情", self)
        layout.addWidget(self.title_label)

        self.empty_label = BodyLabel("尚未选中任何服务器", self)
        self.empty_label.setStyleSheet("color: #8a8a8a;")
        layout.addWidget(self.empty_label)

        self.fields_widget = QWidget(self)
        fields_layout = QVBoxLayout(self.fields_widget)
        fields_layout.setContentsMargins(0, 0, 0, 0)
        fields_layout.setSpacing(6)
        self._field_labels: dict[str, BodyLabel] = {}
        for key, caption in (
            ("name", "名称"),
            ("endpoint", "SSH 端点"),
            ("auth", "认证方式"),
            ("state", "部署状态"),
            ("napcat_version", "NapCat 版本"),
            ("qq_version", "QQ 版本"),
            ("created_at", "创建时间"),
            ("last_connected_at", "最近连接"),
            ("notes", "备注"),
        ):
            row = QHBoxLayout()
            row.setSpacing(8)
            cap = CaptionLabel(f"{caption}:", self.fields_widget)
            cap.setFixedWidth(90)
            cap.setStyleSheet("color: #8a8a8a;")
            value = BodyLabel("--", self.fields_widget)
            value.setWordWrap(True)
            row.addWidget(cap, 0, Qt.AlignmentFlag.AlignTop)
            row.addWidget(value, 1)
            fields_layout.addLayout(row)
            self._field_labels[key] = value

        layout.addWidget(self.fields_widget)
        layout.addStretch()

    def show_empty(self) -> None:
        self._profile = None
        self.empty_label.show()
        self.fields_widget.hide()

    def show_profile(self, profile: ServerProfile) -> None:
        self._profile = profile
        self.empty_label.hide()
        self.fields_widget.show()

        cred = profile.credentials
        state_text, _ = _DEPLOYMENT_LABEL[profile.deployment_state]

        self._field_labels["name"].setText(profile.name)
        self._field_labels["endpoint"].setText(f"{cred.username}@{cred.host}:{cred.port}")
        self._field_labels["auth"].setText("私钥" if cred.auth_method == "key" else "密码")
        self._field_labels["state"].setText(state_text)
        self._field_labels["napcat_version"].setText(profile.napcat_version or "未探测")
        self._field_labels["qq_version"].setText(profile.qq_version or "未探测")
        self._field_labels["created_at"].setText(self._format_timestamp(profile.created_at))
        self._field_labels["last_connected_at"].setText(
            self._format_timestamp(profile.last_connected_at) if profile.last_connected_at else "尚未连接"
        )
        self._field_labels["notes"].setText(profile.notes or "无")

    @staticmethod
    def _format_timestamp(ts: float | None) -> str:
        if not ts:
            return "--"
        return datetime.fromtimestamp(ts).strftime("%Y-%m-%d %H:%M:%S")


# ============================================================
# 主页面
# ============================================================
class RemotePage(QWidget):
    """远程服务器管理页面。"""

    def __init__(self) -> None:
        super().__init__()
        self._selected_id: str | None = None
        self._cards: dict[str, ServerCard] = {}

    def initialize(self, parent: "MainWindow") -> "RemotePage":
        """页面初始化, 由主窗口在创建时调用。"""
        self.setParent(parent)
        self.setObjectName("RemotePage")

        self._build_ui()
        self._connect_manager_signals()
        self._reload()

        PageStyleSheet.REMOTE.apply(self)
        return self

    # ---------- UI 构建 ----------
    def _build_ui(self) -> None:
        root = QVBoxLayout(self)
        root.setContentsMargins(24, 20, 24, 20)
        root.setSpacing(14)

        self.title_label = TitleLabel("远程服务器管理", self)
        self.subtitle_label = CaptionLabel(
            "为 NapCatQQ 添加 Linux 服务器, 即可在远端运行 Bot, 操作体验与本地一致",
            self,
        )
        root.addWidget(self.title_label)
        root.addWidget(self.subtitle_label)

        toolbar = QHBoxLayout()
        toolbar.setSpacing(8)
        self.add_btn = PrimaryPushButton(FI.ADD, "添加服务器", self)
        self.edit_btn = PushButton(FI.EDIT, "编辑", self)
        self.test_btn = PushButton(FI.GLOBE, "测试连接", self)
        self.delete_btn = PushButton(FI.DELETE, "删除", self)
        self.refresh_btn = PushButton(FI.SYNC, "刷新", self)
        toolbar.addWidget(self.add_btn)
        toolbar.addWidget(self.edit_btn)
        toolbar.addWidget(self.test_btn)
        toolbar.addWidget(self.delete_btn)
        toolbar.addStretch()
        toolbar.addWidget(self.refresh_btn)
        root.addLayout(toolbar)

        body = QHBoxLayout()
        body.setSpacing(14)

        list_container = QFrame(self)
        list_container.setObjectName("listContainer")
        list_container.setMinimumWidth(340)
        list_outer = QVBoxLayout(list_container)
        list_outer.setContentsMargins(0, 0, 0, 0)

        self._list_scroll = QScrollArea(list_container)
        self._list_scroll.setWidgetResizable(True)
        self._list_scroll.setFrameShape(QScrollArea.Shape.NoFrame)

        self._list_inner = QWidget(self._list_scroll)
        self._list_layout = QVBoxLayout(self._list_inner)
        self._list_layout.setContentsMargins(0, 0, 0, 0)
        self._list_layout.setSpacing(8)
        self._list_layout.addStretch()
        self._list_scroll.setWidget(self._list_inner)

        list_outer.addWidget(self._list_scroll)
        body.addWidget(list_container, 1)

        self._detail_panel = ServerDetailPanel(self)
        self._detail_panel.setMinimumWidth(360)
        body.addWidget(self._detail_panel, 1)

        root.addLayout(body, 1)

        self._empty_label = BodyLabel(
            "尚未添加任何服务器, 点击左上角 “添加服务器” 开始配置。",
            self,
        )
        self._empty_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._empty_label.setStyleSheet("color: #8a8a8a; padding: 24px;")
        self._list_layout.insertWidget(0, self._empty_label)

        self.add_btn.clicked.connect(self._on_add)
        self.edit_btn.clicked.connect(self._on_edit)
        self.test_btn.clicked.connect(self._on_test)
        self.delete_btn.clicked.connect(self._on_delete)
        self.refresh_btn.clicked.connect(self._reload)

        self._update_button_state()

    def _connect_manager_signals(self) -> None:
        manager = it(ServerManager)
        manager.server_added.connect(lambda *_: self._reload())
        manager.server_updated.connect(lambda *_: self._reload())
        manager.server_removed.connect(lambda *_: self._reload())
        manager.server_state_changed.connect(lambda *_: self._reload())

    # ---------- 列表渲染 ----------
    def _reload(self) -> None:
        manager = it(ServerManager)
        servers = manager.list_servers()

        for card in self._cards.values():
            self._list_layout.removeWidget(card)
            card.deleteLater()
        self._cards.clear()

        if not servers:
            self._empty_label.show()
            self._selected_id = None
            self._detail_panel.show_empty()
            self._update_button_state()
            return

        self._empty_label.hide()
        for index, profile in enumerate(servers):
            card = ServerCard(profile, selected=(profile.id == self._selected_id), parent=self._list_inner)
            self._cards[profile.id] = card
            self._list_layout.insertWidget(index, card)

        if self._selected_id not in self._cards:
            self._selected_id = next(iter(self._cards.keys()))
            self._cards[self._selected_id].set_selected(True)

        selected_profile = manager.get_server(self._selected_id) if self._selected_id else None
        if selected_profile is not None:
            self._detail_panel.show_profile(selected_profile)
        else:
            self._detail_panel.show_empty()
        self._update_button_state()

    def select_server(self, server_id: str) -> None:
        """由 [`ServerCard`](src/desktop/ui/page/remote_page/__init__.py) 点击时调用。"""
        if server_id not in self._cards:
            return
        if self._selected_id and self._selected_id in self._cards:
            self._cards[self._selected_id].set_selected(False)
        self._selected_id = server_id
        self._cards[server_id].set_selected(True)

        manager = it(ServerManager)
        profile = manager.get_server(server_id)
        if profile is not None:
            self._detail_panel.show_profile(profile)
        self._update_button_state()

    def _update_button_state(self) -> None:
        has_selection = self._selected_id is not None and self._selected_id in self._cards
        self.edit_btn.setEnabled(has_selection)
        self.test_btn.setEnabled(has_selection)
        self.delete_btn.setEnabled(has_selection)

    # ---------- 操作回调 ----------
    def _on_add(self) -> None:
        dialog = ServerEditDialog(self.window())
        if dialog.exec():
            try:
                profile = dialog.get_profile()
                password = dialog.get_password()
            except ValueError as exc:
                error_bar(str(exc), parent=self)
                return
            manager = it(ServerManager)
            manager.add_server(profile, password=password)
            self._selected_id = profile.id
            success_bar(f"已添加服务器: {profile.name}", parent=self)

    def _on_edit(self) -> None:
        if not self._selected_id:
            return
        manager = it(ServerManager)
        profile = manager.get_server(self._selected_id)
        if profile is None:
            return
        existing_password = manager._password_cache.get(profile.id)  # noqa: SLF001 - 复用同包私有状态
        dialog = ServerEditDialog(self.window(), profile=profile, existing_password=existing_password)
        if dialog.exec():
            try:
                updated = dialog.get_profile()
                password = dialog.get_password()
            except ValueError as exc:
                error_bar(str(exc), parent=self)
                return
            manager.update_server(updated, password=password)
            success_bar(f"已更新服务器: {updated.name}", parent=self)

    def _on_test(self) -> None:
        if not self._selected_id:
            return
        manager = it(ServerManager)
        profile = manager.get_server(self._selected_id)
        if profile is None:
            return
        password = manager._password_cache.get(profile.id) if profile.credentials.auth_method == "password" else None  # noqa: SLF001
        if profile.credentials.auth_method == "password" and not password:
            error_bar("密码认证模式下未保存密码, 请先编辑并填写密码", parent=self)
            return

        info_bar(f"正在测试连接: {profile.credentials.host}", parent=self)
        self.test_btn.setEnabled(False)

        tester = ConnectionTester(profile, password=password)
        tester.signals.finished.connect(self._on_test_finished)
        QThreadPool.globalInstance().start(tester)

    def _on_test_finished(self, server_id: str, ok: bool, message: str) -> None:
        self._update_button_state()
        manager = it(ServerManager)
        profile = manager.get_server(server_id)
        name = profile.name if profile is not None else server_id
        if ok:
            success_bar(f"[{name}] {message}", parent=self)
            if profile is not None:
                from time import time

                profile.last_connected_at = time()
                manager.update_server(profile)
        else:
            error_bar(f"[{name}] {message}", parent=self)

    def _on_delete(self) -> None:
        if not self._selected_id:
            return
        manager = it(ServerManager)
        profile = manager.get_server(self._selected_id)
        if profile is None:
            return

        ask = AskBox(
            "确认删除",
            f"确定要删除服务器 “{profile.name}” 吗？\n该操作不会影响远端已部署的 NapCat。",
            self.window(),
        )
        if ask.exec():
            removed = manager.remove_server(profile.id)
            if removed:
                self._selected_id = None
                info_bar(f"已删除服务器: {profile.name}", parent=self)


class RemotePageCreator(AbstractCreator, ABC):
    """远程页面创建器。"""

    targets = (CreateTargetInfo("src.desktop.ui.page.remote_page", "RemotePage"),)

    @staticmethod
    def available() -> bool:
        return exists_module("src.desktop.ui.page.remote_page")

    def create(self, *args, **kwargs) -> RemotePage:
        return RemotePage(*args, **kwargs)


add_creator(RemotePageCreator)
