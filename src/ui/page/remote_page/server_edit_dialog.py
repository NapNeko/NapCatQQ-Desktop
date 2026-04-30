# -*- coding: utf-8 -*-
"""服务器档案添加 / 编辑对话框。

基于 [`MessageBoxBase`](https://qfluentwidgets.com/) 实现, 校验 SSH 字段,
返回新的 [`ServerProfile`](src/core/remote/servers.py) 与可选密码。
"""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

from PySide6.QtCore import QDir
from PySide6.QtWidgets import QFileDialog, QHBoxLayout, QVBoxLayout, QWidget
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    ComboBox,
    LineEdit,
    MessageBoxBase,
    StrongBodyLabel,
    SubtitleLabel,
    ToolButton,
    FluentIcon as FI,
)

from src.core.remote import ServerProfile, SSHCredentials
from src.core.remote.ssh_keys import scan_local_ssh_keys

if TYPE_CHECKING:
    pass


class ServerEditDialog(MessageBoxBase):
    """添加 / 编辑服务器档案。

    Args:
        parent: 父级窗口
        profile: 编辑模式下传入待修改的档案; None 表示新建
        existing_password: 编辑模式下从 [`ServerManager._password_cache`](src/core/remote/server_manager.py) 注入

    成功 ``exec()`` 后通过 [`get_profile`](src/ui/page/remote_page/server_edit_dialog.py)
    与 [`get_password`](src/ui/page/remote_page/server_edit_dialog.py) 取回结果。
    """

    def __init__(
        self,
        parent: QWidget,
        *,
        profile: ServerProfile | None = None,
        existing_password: str | None = None,
    ) -> None:
        super().__init__(parent=parent)
        self._original_profile = profile
        self._setup_ui()
        self._connect_signals()
        self._load_from_profile(profile, existing_password)
        self.widget.setMinimumSize(520, 540)

    # ==================== UI 构建 ====================
    def _setup_ui(self) -> None:
        title_text = "编辑服务器" if self._original_profile is not None else "添加服务器"
        self.title_label = SubtitleLabel(title_text, self)
        self.caption_label = CaptionLabel("配置远程 Linux 服务器的 SSH 接入信息", self)
        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addWidget(self.caption_label)

        # ---------- 基本信息 ----------
        basic_block = self._create_section("基本信息")
        basic_layout: QVBoxLayout = basic_block.layout()  # type: ignore[assignment]

        self.name_edit = LineEdit(basic_block)
        self.name_edit.setPlaceholderText("服务器显示名（如：腾讯云-香港）")
        basic_layout.addLayout(self._row("名称:", self.name_edit, label_width=50))

        self.host_edit = LineEdit(basic_block)
        self.host_edit.setPlaceholderText("192.168.1.100")
        self.port_edit = LineEdit(basic_block)
        self.port_edit.setText("22")
        self.port_edit.setFixedWidth(80)
        host_row = QHBoxLayout()
        host_row.setSpacing(10)
        host_row.addLayout(self._row("主机:", self.host_edit, label_width=50), 3)
        host_row.addLayout(self._row("端口:", self.port_edit, label_width=40), 1)
        basic_layout.addLayout(host_row)

        self.username_edit = LineEdit(basic_block)
        self.username_edit.setPlaceholderText("root / ubuntu")
        basic_layout.addLayout(self._row("用户名:", self.username_edit, label_width=50))

        self.viewLayout.addWidget(basic_block)

        # ---------- 认证 ----------
        auth_block = self._create_section("认证方式")
        auth_layout: QVBoxLayout = auth_block.layout()  # type: ignore[assignment]

        self.method_combo = ComboBox(auth_block)
        self.method_combo.addItem("私钥", userData="key")
        self.method_combo.addItem("密码", userData="password")
        self.method_combo.setFixedWidth(120)
        method_container = QHBoxLayout()
        method_container.addWidget(BodyLabel("方式:", auth_block))
        method_container.addWidget(self.method_combo)
        method_container.addStretch()
        auth_layout.addLayout(method_container)

        # 私钥: LineEdit (主要输入) + 扫描候选下拉 (快捷选择) + 浏览按钮
        # 不使用 EditableComboBox: 其多重继承 (LineEdit + ComboBoxBase) 行为太难推断。
        self.key_widget = QWidget(auth_block)
        key_row = QHBoxLayout(self.key_widget)
        key_row.setContentsMargins(0, 0, 0, 0)
        key_row.setSpacing(6)

        self.key_edit = LineEdit(self.key_widget)
        self.key_edit.setClearButtonEnabled(True)

        scanned_keys = scan_local_ssh_keys()
        self._scanned_keys: tuple[str, ...] = tuple(scanned_keys)

        if scanned_keys:
            self.key_edit.setText(scanned_keys[0])
            self.key_edit.setPlaceholderText("已默认填入优先密钥, 可手动修改")
        else:
            self.key_edit.setPlaceholderText("~/.ssh/id_rsa（未扫描到本地密钥, 请手输或浏览）")

        self.key_scan_combo = ComboBox(self.key_widget)
        self.key_scan_combo.setFixedWidth(120)
        self.key_scan_combo.setToolTip("从本地扫描到的 SSH 私钥快捷选择")
        if scanned_keys:
            for path in scanned_keys:
                self.key_scan_combo.addItem(Path(path).name, userData=path)
            self.key_scan_combo.currentIndexChanged.connect(self._on_scan_combo_changed)
        else:
            self.key_scan_combo.addItem("(无扫描结果)")
            self.key_scan_combo.setEnabled(False)

        self.key_browse_btn = ToolButton(FI.FOLDER, self.key_widget)
        self.key_browse_btn.setFixedSize(32, 32)
        self.key_browse_btn.setToolTip("浏览选择其他位置的私钥文件")

        key_row.addWidget(BodyLabel("私钥:", self.key_widget))
        key_row.addWidget(self.key_edit, 1)
        key_row.addWidget(self.key_scan_combo)
        key_row.addWidget(self.key_browse_btn)
        auth_layout.addWidget(self.key_widget)

        if scanned_keys:
            scan_hint = CaptionLabel(
                f"已默认填入本地扫描到的 {len(scanned_keys)} 个密钥中优先级最高的一个, 可点击中间按钮切换",
                auth_block,
            )
            scan_hint.setStyleSheet("color: #8a8a8a;")
            auth_layout.addWidget(scan_hint)

        # 密码
        self.pwd_widget = QWidget(auth_block)
        pwd_row = QHBoxLayout(self.pwd_widget)
        pwd_row.setContentsMargins(0, 0, 0, 0)
        pwd_row.setSpacing(6)
        self.pwd_edit = LineEdit(self.pwd_widget)
        self.pwd_edit.setEchoMode(LineEdit.EchoMode.Password)
        self.pwd_edit.setPlaceholderText("登录密码（仅保存到内存, 不写入磁盘）")
        pwd_row.addWidget(BodyLabel("密码:", self.pwd_widget))
        pwd_row.addWidget(self.pwd_edit, 1)
        auth_layout.addWidget(self.pwd_widget)
        self.pwd_widget.hide()

        self.viewLayout.addWidget(auth_block)

        # ---------- 高级 ----------
        adv_block = self._create_section("高级选项")
        adv_layout: QVBoxLayout = adv_block.layout()  # type: ignore[assignment]

        self.timeout_edit = LineEdit(adv_block)
        self.timeout_edit.setText("10")
        self.timeout_edit.setFixedWidth(80)
        self.cmd_timeout_edit = LineEdit(adv_block)
        self.cmd_timeout_edit.setText("20")
        self.cmd_timeout_edit.setFixedWidth(80)
        timeout_row = QHBoxLayout()
        timeout_row.setSpacing(10)
        timeout_row.addLayout(self._row("连接超时(秒):", self.timeout_edit, label_width=90), 1)
        timeout_row.addLayout(self._row("命令超时(秒):", self.cmd_timeout_edit, label_width=90), 1)
        adv_layout.addLayout(timeout_row)

        self.policy_combo = ComboBox(adv_block)
        self.policy_combo.addItem("严格检查（推荐）", userData="reject")
        self.policy_combo.addItem("警告（不推荐）", userData="warning")
        self.policy_combo.addItem("自动添加（仅测试环境）", userData="auto_add")
        self.policy_combo.setFixedWidth(220)
        policy_container = QHBoxLayout()
        policy_container.addWidget(BodyLabel("主机指纹策略:", adv_block))
        policy_container.addWidget(self.policy_combo)
        policy_container.addStretch()
        adv_layout.addLayout(policy_container)

        self.notes_edit = LineEdit(adv_block)
        self.notes_edit.setPlaceholderText("备注（可选）")
        adv_layout.addLayout(self._row("备注:", self.notes_edit, label_width=50))

        self.viewLayout.addWidget(adv_block)

        # 错误提示
        self.error_label = CaptionLabel("", self)
        self.error_label.setStyleSheet("color: #d83b01;")
        self.error_label.hide()
        self.viewLayout.addWidget(self.error_label)

    def _connect_signals(self) -> None:
        self.method_combo.currentIndexChanged.connect(self._on_method_changed)
        self.key_browse_btn.clicked.connect(self._on_browse_key)

    @staticmethod
    def _row(label_text: str, widget: QWidget, *, label_width: int) -> QHBoxLayout:
        row = QHBoxLayout()
        row.setSpacing(6)
        label = BodyLabel(label_text, widget.parentWidget() or widget)
        label.setFixedWidth(label_width)
        row.addWidget(label)
        row.addWidget(widget, 1)
        return row

    def _create_section(self, title: str) -> QWidget:
        wrapper = QWidget(self)
        layout = QVBoxLayout(wrapper)
        layout.setContentsMargins(0, 6, 0, 0)
        layout.setSpacing(8)
        layout.addWidget(StrongBodyLabel(title, wrapper))
        return wrapper

    # ==================== 数据加载 ====================
    def _load_from_profile(self, profile: ServerProfile | None, existing_password: str | None) -> None:
        if profile is None:
            return
        cred = profile.credentials
        self.name_edit.setText(profile.name)
        self.host_edit.setText(cred.host)
        self.port_edit.setText(str(cred.port))
        self.username_edit.setText(cred.username)

        method_index = self.method_combo.findData(cred.auth_method)
        if method_index >= 0:
            self.method_combo.setCurrentIndex(method_index)
        self._apply_method_visibility(cred.auth_method)

        if cred.private_key_path:
            self.key_edit.setText(cred.private_key_path)
        if existing_password:
            self.pwd_edit.setText(existing_password)

        self.timeout_edit.setText(str(int(cred.connect_timeout)))
        self.cmd_timeout_edit.setText(str(int(cred.command_timeout)))

        policy_index = self.policy_combo.findData(cred.host_key_policy)
        if policy_index >= 0:
            self.policy_combo.setCurrentIndex(policy_index)

        self.notes_edit.setText(profile.notes)

    # ==================== 信号回调 ====================
    def _on_method_changed(self, index: int) -> None:
        self._apply_method_visibility(self.method_combo.itemData(index))

    def _apply_method_visibility(self, method: str) -> None:
        self.key_widget.setVisible(method == "key")
        self.pwd_widget.setVisible(method == "password")

    def _on_scan_combo_changed(self, index: int) -> None:
        """扫描候选选中 → 同步到主 LineEdit。"""
        if index < 0:
            return
        path = self.key_scan_combo.itemData(index)
        if isinstance(path, str) and path:
            self.key_edit.setText(path)

    def _on_browse_key(self) -> None:
        file_path, _ = QFileDialog.getOpenFileName(
            self,
            "选择 SSH 私钥",
            QDir.homePath(),
            "私钥文件 (*.pem id_rsa id_ed25519 *);;所有文件 (*)",
        )
        if file_path:
            self.key_edit.setText(file_path)

    # ==================== 表单校验与结果 ====================
    def validate(self) -> bool:  # noqa: D401 - 重写 MessageBoxBase 钩子
        """点击 yes 时被 MessageBoxBase 调用; 返回 False 则阻止关闭。"""
        try:
            self._build_credentials_or_raise()
        except ValueError as exc:
            self.error_label.setText(str(exc))
            self.error_label.show()
            return False
        self.error_label.hide()
        return True

    def get_profile(self) -> ServerProfile:
        """构造并返回新的 [`ServerProfile`](src/core/remote/servers.py)。

        编辑模式下复用原 ``id`` / ``created_at`` / ``deployment_state`` / 探测缓存,
        新建模式下生成全新的 ``id``。
        """
        credentials = self._build_credentials_or_raise()
        name = self.name_edit.text().strip() or credentials.host
        notes = self.notes_edit.text().strip()

        if self._original_profile is None:
            return ServerProfile.create(name=name, credentials=credentials, notes=notes)

        original = self._original_profile
        return ServerProfile(
            id=original.id,
            name=name,
            credentials=credentials,
            paths=original.paths,
            deployment_state=original.deployment_state,
            napcat_version=original.napcat_version,
            qq_version=original.qq_version,
            notes=notes,
            created_at=original.created_at,
            last_connected_at=original.last_connected_at,
        )

    def get_password(self) -> str | None:
        """获取本次输入的密码; 仅密码认证模式下有效。"""
        if self.method_combo.currentData() != "password":
            return None
        password = self.pwd_edit.text()
        return password or None

    # ==================== 内部 ====================
    def _build_credentials_or_raise(self) -> SSHCredentials:
        host = self.host_edit.text().strip()
        if not host:
            raise ValueError("主机地址不能为空")

        try:
            port = int(self.port_edit.text() or "22")
        except ValueError as exc:
            raise ValueError("端口必须是数字") from exc
        if port <= 0 or port > 65535:
            raise ValueError("端口范围应在 1-65535")

        username = self.username_edit.text().strip()
        if not username:
            raise ValueError("用户名不能为空")

        auth_method = self.method_combo.currentData()
        password = self.pwd_edit.text() if auth_method == "password" else None
        private_key_path = self.key_edit.text().strip() if auth_method == "key" else None

        if auth_method == "password" and not password:
            raise ValueError("密码认证模式下必须填写密码")
        if auth_method == "key" and not private_key_path:
            raise ValueError("私钥认证模式下必须选择私钥文件")

        try:
            connect_timeout = float(self.timeout_edit.text() or "10")
            command_timeout = float(self.cmd_timeout_edit.text() or "20")
        except ValueError as exc:
            raise ValueError("超时时间必须是数字") from exc
        if connect_timeout <= 0 or command_timeout <= 0:
            raise ValueError("超时时间必须大于 0")

        credentials = SSHCredentials(
            host=host,
            port=port,
            username=username,
            auth_method=auth_method,
            password=password,
            private_key_path=private_key_path,
            connect_timeout=connect_timeout,
            command_timeout=command_timeout,
            host_key_policy=self.policy_combo.currentData(),
        )
        # 复用现有 SSHCredentials.validate 进行二次校验
        try:
            credentials.validate()
        except ValueError:
            # validate() 自带中文提示, 但密钥文件不存在的提示对用户更友好, 这里直接抛出
            raise
        return credentials
