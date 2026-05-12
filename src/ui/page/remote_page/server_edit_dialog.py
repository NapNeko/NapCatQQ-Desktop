# -*- coding: utf-8 -*-
"""服务器档案添加 / 编辑对话框. 

基于 [`MessageBoxBase`](https://qfluentwidgets.com/) 实现, 校验 SSH 字段,
返回新的 [`ServerProfile`](src/core/remote/servers.py) 与可选密码. 

UI 组织 (v2 统一表单布局):
- 顶部: 标题 + 描述
- 3 个分组 (基本信息 / 认证方式 / 高级选项), 每组内部使用 [`QFormLayout`] 统一标签列宽与右对齐
- 认证方式行根据下拉选择动态显示 "私钥文件+浏览+扫描快捷" 或 "登录密码"
- 错误提示在对话框底部作为 CaptionLabel 出现
"""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

from PySide6.QtCore import QDir, Qt
from PySide6.QtGui import QDragEnterEvent, QDragMoveEvent, QDropEvent
from PySide6.QtWidgets import QFileDialog, QFormLayout, QHBoxLayout, QVBoxLayout, QWidget
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    CheckBox,
    ComboBox,
    LineEdit,
    MessageBoxBase,
    StrongBodyLabel,
    SubtitleLabel,
    ToolButton,
    ToolTipFilter,
    FluentIcon as FI,
)

from src.core.remote import BackendFlavor, ServerProfile, SSHCredentials
from src.core.remote.credential_store import CredentialStore
from src.core.remote.snowluma import SnowLumaRemotePaths
from src.core.remote.ssh_keys import scan_local_ssh_keys

if TYPE_CHECKING:
    pass


class _PrivateKeyDropLineEdit(LineEdit):
    """支持文件拖拽的私钥路径输入框 (P4 F5.3).

    接受规则:

    - 单个文件 (非目录)
    - 文件名命中 ``id_rsa`` / ``id_ed25519`` / ``id_dsa`` / ``id_ecdsa`` /
      ``*.pem`` / ``*.ppk`` 之一, **或** 文件首行以 ``-----BEGIN`` 开头 (PEM 头部)
    - 多文件 / 目录 / 不命中规则: 弹用户提示, 不修改输入框

    drop 完成后通过 ``setText`` 写入路径; 调用方应监听 ``textChanged`` 触发
    后续校验.
    """

    _ACCEPT_NAMES: tuple[str, ...] = ("id_rsa", "id_ed25519", "id_dsa", "id_ecdsa")
    _ACCEPT_SUFFIXES: tuple[str, ...] = (".pem", ".ppk", ".key")
    _PEM_HEADERS: tuple[bytes, ...] = (
        b"-----BEGIN RSA PRIVATE KEY-----",
        b"-----BEGIN OPENSSH PRIVATE KEY-----",
        b"-----BEGIN PRIVATE KEY-----",
        b"-----BEGIN EC PRIVATE KEY-----",
        b"-----BEGIN DSA PRIVATE KEY-----",
        b"-----BEGIN ENCRYPTED PRIVATE KEY-----",
    )

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setAcceptDrops(True)

    # ==================== 拖拽事件 ====================
    def dragEnterEvent(self, event: QDragEnterEvent) -> None:
        if event.mimeData().hasUrls():
            event.acceptProposedAction()
        else:
            event.ignore()

    def dragMoveEvent(self, event: QDragMoveEvent) -> None:
        if event.mimeData().hasUrls():
            event.acceptProposedAction()

    def dropEvent(self, event: QDropEvent) -> None:
        urls = event.mimeData().urls()
        local_paths = [url.toLocalFile() for url in urls]
        accepted_path, hint = self.evaluate_drop_paths(local_paths)
        if accepted_path is None:
            if hint:
                self._show_hint(hint)
            event.ignore()
            return

        self.setText(accepted_path)
        event.acceptProposedAction()

    @classmethod
    def evaluate_drop_paths(cls, local_paths: list[str]) -> tuple[str | None, str]:
        """纯逻辑层判定: 接受单个私钥文件路径, 返回 ``(accepted_path, hint)``.

        ``accepted_path`` 为 ``None`` 时表示拒绝, 同时 ``hint`` 是给用户的提示文案;
        接受时 ``hint`` 为空串. 把这块逻辑独立出来主要是为了避免在测试里构造易崩
        的 ``QDropEvent``.
        """
        if not local_paths or len(local_paths) != 1:
            return None, "请拖入单个私钥文件"
        local_path = local_paths[0]
        if not local_path:
            return None, "不支持的拖拽来源, 请选择本地文件"
        path_obj = Path(local_path)
        if not path_obj.is_file():
            return None, "请拖入单个私钥文件 (而非目录)"
        if not cls._looks_like_private_key(path_obj):
            return None, "无法识别为 SSH 私钥, 请确认文件内容"
        return str(path_obj), ""

    # ==================== 辅助 ====================
    @classmethod
    def _looks_like_private_key(cls, path: Path) -> bool:
        """启发式判断: 文件名 / 后缀 / PEM 头部 三选一命中即可."""
        name = path.name.lower()
        if name in cls._ACCEPT_NAMES:
            return True
        if any(name.endswith(suffix) for suffix in cls._ACCEPT_SUFFIXES):
            return True
        try:
            with path.open("rb") as fp:
                head = fp.read(64)
        except OSError:
            return False
        return any(head.startswith(header) for header in cls._PEM_HEADERS)

    def _show_hint(self, message: str) -> None:
        """非侵入式提示: 走 InfoBar; 失败时容忍 (无 QApplication 上下文等)."""
        try:
            from src.ui.components.info_bar import warning_bar

            warning_bar(message, parent=self)
        except Exception:  # noqa: BLE001
            # 测试环境无 InfoBar 上下文时静默
            pass


class ServerEditDialog(MessageBoxBase):
    """添加 / 编辑服务器档案. 

    Args:
        parent: 父级窗口
        profile: 编辑模式下传入待修改的档案; None 表示新建
        existing_password: 编辑模式下从 [`ServerManager._password_cache`](src/core/remote/server_manager.py) 注入

    成功 ``exec()`` 后通过 [`get_profile`](src/ui/page/remote_page/server_edit_dialog.py)
    与 [`get_password`](src/ui/page/remote_page/server_edit_dialog.py) 取回结果. 
    """

    # 表单标签列的统一宽度: 让各分组的标签列纵向对齐
    # (W10b: 恢复到 92, 确保 "主机指纹策略" 等较长标签完整显示)
    _LABEL_COLUMN_WIDTH = 92

    def __init__(
        self,
        parent: QWidget,
        *,
        profile: ServerProfile | None = None,
        existing_password: str | None = None,
        credential_store: CredentialStore | None = None,
        existing_remember_password: bool = False,
    ) -> None:
        super().__init__(parent=parent)
        self._original_profile = profile
        # 认证分组的 form 引用, 用于 setRowVisible 切换认证方式
        self._auth_form: QFormLayout | None = None
        self._auth_row_key: int = -1
        self._auth_row_pwd: int = -1
        self._auth_row_remember: int = -1
        self._auth_row_auto_key: int = -1
        # W10b: SnowLuma 分组 (独立 section_title + form); 用 widget 引用整体 show/hide
        self._sl_section_title: StrongBodyLabel | None = None
        self._sl_form_widget: QWidget | None = None
        # 私钥输入模式: False = 下拉扫描候选 combo; True = 手动路径 LineEdit
        self._key_manual_mode: bool = False
        # P4 F5.2: keyring 探测; 不可用时 "记住密码" 勾选项隐藏
        self._credential_store: CredentialStore = credential_store if credential_store is not None else CredentialStore()
        self._existing_remember_password: bool = existing_remember_password

        self._setup_ui()
        self._connect_signals()
        self._load_from_profile(profile, existing_password)
        # W10b v2: 两栏布局 -> 总宽 900 (左右每栏 ~410), 高度 540 (两栏并排后变矮)
        self.widget.setMinimumSize(900, 540)

    # ==================== UI 构建 ====================
    def _setup_ui(self) -> None:
        title_text = "编辑服务器" if self._original_profile is not None else "添加服务器"
        self.title_label = SubtitleLabel(title_text, self)
        self.caption_label = CaptionLabel("配置远程 Linux 服务器的 SSH 接入信息", self)
        self.caption_label.setObjectName("serverEditCaption")
        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addWidget(self.caption_label)
        self.viewLayout.addSpacing(2)

        # ---------------- 两栏容器 (W10b v2) ----------------
        # 左栏: 部署类型 + 基本信息 + 认证方式 (必填项)
        # 右栏: 高级选项 + SnowLuma 选项 (可选项 / flavor 依赖项)
        columns_widget = QWidget(self)
        columns_widget.setObjectName("serverEditColumns")
        columns_layout = QHBoxLayout(columns_widget)
        columns_layout.setContentsMargins(0, 0, 0, 0)
        columns_layout.setSpacing(24)

        left_col = QWidget(columns_widget)
        left_col.setObjectName("serverEditLeftCol")
        self._left_col_layout = QVBoxLayout(left_col)
        self._left_col_layout.setContentsMargins(0, 0, 0, 0)
        self._left_col_layout.setSpacing(0)

        right_col = QWidget(columns_widget)
        right_col.setObjectName("serverEditRightCol")
        self._right_col_layout = QVBoxLayout(right_col)
        self._right_col_layout.setContentsMargins(0, 0, 0, 0)
        self._right_col_layout.setSpacing(0)

        columns_layout.addWidget(left_col, 1)
        columns_layout.addWidget(right_col, 1)
        self.viewLayout.addWidget(columns_widget, 1)

        # ---------------- 部署类型 (左栏) ----------------
        # 必须放在左栏最上面: flavor 决定右栏 SnowLuma 分组是否可见
        flavor_form = self._add_section("部署类型", target=self._left_col_layout)
        self.flavor_combo = ComboBox(self)
        self.flavor_combo.addItem("NapCat (原生二进制)", userData=BackendFlavor.NAPCAT.value)
        self.flavor_combo.addItem("SnowLuma (远端框架)", userData=BackendFlavor.SNOWLUMA.value)
        # 留最小宽度 240, 不锁死上限, 让 form 的 AllNonFixedFieldsGrow 带着填满可用宽度
        self.flavor_combo.setMinimumWidth(240)
        self.flavor_combo.setToolTip(
            "NapCat: 在远端 Linux 直接装 LinuxQQ + NapCat, 无需图形栈\n"
            "SnowLuma: 远端跑完整 SnowLuma.Framework (Xvfb + noVNC 扫码登录),\n"
            "适合需要稳定登录态与远端图形化扫码的场景"
        )
        flavor_form.addRow(self._make_label("后端类型"), self.flavor_combo)

        # ---------------- 基本信息 (左栏) ----------------
        basic_form = self._add_section("基本信息", target=self._left_col_layout)

        self.name_edit = LineEdit(self)
        self.name_edit.setPlaceholderText("服务器显示名 (如: 腾讯云-香港)")

        # 主机 + 端口 合成为单行 (host 占主, port 固定窄)
        host_port_widget = QWidget(self)
        host_port_row = QHBoxLayout(host_port_widget)
        host_port_row.setContentsMargins(0, 0, 0, 0)
        host_port_row.setSpacing(8)
        self.host_edit = LineEdit(host_port_widget)
        self.host_edit.setPlaceholderText("192.168.1.100 或 domain.example.com")
        self.port_edit = LineEdit(host_port_widget)
        self.port_edit.setText("22")
        self.port_edit.setFixedWidth(72)
        self.port_edit.setAlignment(Qt.AlignmentFlag.AlignCenter)
        port_label = BodyLabel(":", host_port_widget)
        host_port_row.addWidget(self.host_edit, 1)
        host_port_row.addWidget(port_label, 0, Qt.AlignmentFlag.AlignVCenter)
        host_port_row.addWidget(self.port_edit, 0, Qt.AlignmentFlag.AlignVCenter)

        self.username_edit = LineEdit(self)
        self.username_edit.setPlaceholderText("root / 用户名")

        basic_form.addRow(self._make_label("名称"), self.name_edit)
        basic_form.addRow(self._make_label("主机 / 端口"), host_port_widget)
        basic_form.addRow(self._make_label("用户名"), self.username_edit)

        # ---------------- 认证方式 (左栏) ----------------
        auth_form = self._add_section("认证方式", target=self._left_col_layout)
        self._auth_form = auth_form

        self.method_combo = ComboBox(self)
        self.method_combo.addItem("私钥", userData="key")
        self.method_combo.addItem("密码", userData="password")
        # 留最小宽度 160, 不锁死上限, 跟随 form 填充
        self.method_combo.setMinimumWidth(160)

        # 私钥行: 自适应 (有扫描结果 → 下拉; 手动浏览后 → 输入框) + 浏览/返回按钮
        scanned_keys = scan_local_ssh_keys()
        self._scanned_keys: tuple[str, ...] = tuple(scanned_keys)

        key_widget = QWidget(self)
        key_row = QHBoxLayout(key_widget)
        key_row.setContentsMargins(0, 0, 0, 0)
        key_row.setSpacing(6)

        # 扫描候选下拉 (combo 模式)
        self.key_combo = ComboBox(key_widget)
        self.key_combo.setToolTip("本地扫描到的 SSH 私钥, 点选即用")
        for path in scanned_keys:
            self.key_combo.addItem(Path(path).name, userData=path)

        # 手动路径 (manual 模式) - P4 F5.3: 支持文件拖拽
        self.key_edit = _PrivateKeyDropLineEdit(key_widget)
        self.key_edit.setClearButtonEnabled(True)
        self.key_edit.setPlaceholderText("~/.ssh/id_rsa  (可拖入文件)")

        # 浏览按钮 (任何模式都可点, 用于切到 manual 模式或重新选择)
        self.key_browse_btn = ToolButton(FI.FOLDER, key_widget)
        self.key_browse_btn.setFixedSize(32, 32)
        self.key_browse_btn.setToolTip("手动选择私钥文件")
        self.key_browse_btn.setToolTipDuration(1500)
        self.key_browse_btn.installEventFilter(ToolTipFilter(self.key_browse_btn, showDelay=300))

        # 返回扫描候选按钮 (仅 manual 模式且存在扫描结果时可见)
        self.key_scan_btn = ToolButton(FI.HISTORY, key_widget)
        self.key_scan_btn.setFixedSize(32, 32)
        self.key_scan_btn.setToolTip("返回扫描候选")
        self.key_scan_btn.setToolTipDuration(1500)
        self.key_scan_btn.installEventFilter(ToolTipFilter(self.key_scan_btn, showDelay=300))

        key_row.addWidget(self.key_combo, 1)
        key_row.addWidget(self.key_edit, 1)
        key_row.addWidget(self.key_browse_btn, 0)
        key_row.addWidget(self.key_scan_btn, 0)

        # 初始模式: 有扫描结果 → combo 模式; 否则 → manual 模式
        if scanned_keys:
            self._set_key_mode(manual=False)
        else:
            self._set_key_mode(manual=True)

        # 密码行
        self.pwd_edit = LineEdit(self)
        self.pwd_edit.setEchoMode(LineEdit.EchoMode.Password)
        self.pwd_edit.setPlaceholderText("登录密码 (仅保存到内存, 不写入磁盘)")

        # P4 F5.2: "记住密码" 勾选行; keyring 不可用时整行隐藏
        self.remember_check = CheckBox("记住密码 (保存到 Windows Credential Manager)", self)
        self.remember_check.setChecked(False)
        keyring_available = self._credential_store.is_available()
        if not keyring_available:
            self.remember_check.setEnabled(False)
            self.remember_check.setToolTip(
                "当前环境不支持 Credential Manager (keyring), 仅当本次会话内可用"
            )

        # "自动配置 SSH 密钥" 勾选行: 仅在密码模式下出现, 含义对齐 ssh-copy-id;
        # 勾选后, 用户保存档案时由 RemotePage 触发一次密码登录 + 公钥下发的后台任务,
        # 成功后档案会自动切到密钥认证. 失败保持密码认证, 用户可重试.
        self.auto_key_check = CheckBox("登录后自动配置 SSH 密钥, 实现免密登录", self)
        self.auto_key_check.setChecked(False)
        self.auto_key_check.setToolTip(
            "勾选后保存档案时会用本次密码登录一次远端, 把本地 ~/.ssh/id_ed25519 的公钥\n"
            "幂等追加到远端 ~/.ssh/authorized_keys (类似 ssh-copy-id), 然后档案切到密钥认证."
        )

        # 添加认证行并记录索引
        auth_form.addRow(self._make_label("认证方式"), self.method_combo)
        self._auth_row_key = auth_form.rowCount()
        auth_form.addRow(self._make_label("私钥"), key_widget)
        self._auth_row_pwd = auth_form.rowCount()
        auth_form.addRow(self._make_label("登录密码"), self.pwd_edit)
        self._auth_row_remember = auth_form.rowCount()
        auth_form.addRow(self._make_label(""), self.remember_check)
        self._auth_row_auto_key = auth_form.rowCount()
        auth_form.addRow(self._make_label(""), self.auto_key_check)

        # 初始可见性: 默认 "私钥" 模式
        auth_form.setRowVisible(self._auth_row_pwd, False)
        auth_form.setRowVisible(self._auth_row_remember, False)
        auth_form.setRowVisible(self._auth_row_auto_key, False)

        # ---------------- 高级选项 (右栏) ----------------
        adv_form = self._add_section("高级选项", target=self._right_col_layout)

        # 两个超时合并一行 (label 固定标识 / 两个小输入框共享)
        timeout_widget = QWidget(self)
        timeout_row = QHBoxLayout(timeout_widget)
        timeout_row.setContentsMargins(0, 0, 0, 0)
        timeout_row.setSpacing(12)
        self.timeout_edit = LineEdit(timeout_widget)
        self.timeout_edit.setText("10")
        self.timeout_edit.setFixedWidth(64)
        self.timeout_edit.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.cmd_timeout_edit = LineEdit(timeout_widget)
        self.cmd_timeout_edit.setText("20")
        self.cmd_timeout_edit.setFixedWidth(64)
        self.cmd_timeout_edit.setAlignment(Qt.AlignmentFlag.AlignCenter)
        # W10b: 将 "连接" / "命令" 上下文与单位 "秒" 拆开留多点空间, 避免被挤出
        timeout_row.addWidget(BodyLabel("连接", timeout_widget))
        timeout_row.addWidget(self.timeout_edit)
        timeout_row.addWidget(BodyLabel("秒", timeout_widget))
        timeout_row.addSpacing(12)
        timeout_row.addWidget(BodyLabel("命令", timeout_widget))
        timeout_row.addWidget(self.cmd_timeout_edit)
        timeout_row.addWidget(BodyLabel("秒", timeout_widget))
        timeout_row.addStretch(1)

        # 主机指纹策略下拉.
        # "interactive" 是新服务器的默认: 首次连接弹 HostKeyConfirmDialog 让用户看
        # SHA256 指纹后选 记忆/仅本次/拒绝, 之后已记忆指纹由 paramiko 自身校验,
        # 真正变更才会以 BadHostKeyException 报"指纹不匹配". 这条选项对应
        # P4 F5.1 引入的 ``HostKeyPolicy="interactive"``, 需要 MainWindow 启动期
        # 调用 ``bootstrap_host_key_dialog`` 把 UI 回调注册进去, 否则会兜底为拒绝.
        self.policy_combo = ComboBox(self)
        self.policy_combo.addItem("首次连接确认 (推荐)", userData="interactive")
        self.policy_combo.addItem("严格检查 (仅信任已记忆指纹)", userData="reject")
        self.policy_combo.addItem("警告 (不推荐)", userData="warning")
        self.policy_combo.addItem("自动添加 (仅测试环境)", userData="auto_add")

        self.notes_edit = LineEdit(self)
        self.notes_edit.setPlaceholderText("备注 (可选)")

        adv_form.addRow(self._make_label("超时设置"), timeout_widget)
        adv_form.addRow(self._make_label("主机指纹策略"), self.policy_combo)
        adv_form.addRow(self._make_label("备注"), self.notes_edit)

        # ---------------- SnowLuma 选项 (右栏, W10b, 仅 SL flavor 可见) ----------------
        sl_section_title = StrongBodyLabel("SnowLuma 选项", self)
        sl_section_title.setObjectName("serverEditSectionTitle")
        self._right_col_layout.addSpacing(2)
        self._right_col_layout.addWidget(sl_section_title)
        self._sl_section_title = sl_section_title

        sl_form_widget = QWidget(self)
        sl_form_widget.setObjectName("serverEditSlFormWidget")
        sl_form = QFormLayout(sl_form_widget)
        sl_form.setContentsMargins(0, 0, 0, 0)
        sl_form.setHorizontalSpacing(10)
        sl_form.setVerticalSpacing(10)
        sl_form.setLabelAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        sl_form.setFieldGrowthPolicy(QFormLayout.FieldGrowthPolicy.AllNonFixedFieldsGrow)

        self.sl_base_dir_edit = LineEdit(sl_form_widget)
        self.sl_base_dir_edit.setText("$HOME/snowluma-remote")
        self.sl_base_dir_edit.setPlaceholderText("SnowLuma 远端工作目录 (默认 $HOME/snowluma-remote)")
        self.sl_base_dir_edit.setToolTip(
            "SnowLuma 在远端机器上的根目录. 下方会自动派生 workspace / runtime / log 等子目录.\n"
            "禁止 shell 元字符 (空格除外); 支持 $HOME 前缀."
        )

        self.sl_webui_pwd_edit = LineEdit(sl_form_widget)
        self.sl_webui_pwd_edit.setEchoMode(LineEdit.EchoMode.Password)
        self.sl_webui_pwd_edit.setClearButtonEnabled(True)
        self.sl_webui_pwd_edit.setPlaceholderText("WebUI 密码 override (留空走 SHA256 fallback)")
        self.sl_webui_pwd_edit.setToolTip(
            "SnowLuma WebUI 登录密码; 留空则由 App 根据 webui.secret 生成 fallback.\n"
            "明文保存到 servers.json (与 NC 档案一致策略), 如需更强保护请使用 keyring."
        )

        sl_form.addRow(self._make_label("工作目录"), self.sl_base_dir_edit)
        sl_form.addRow(self._make_label("WebUI 密码"), self.sl_webui_pwd_edit)

        self._right_col_layout.addWidget(sl_form_widget)
        self._sl_form_widget = sl_form_widget
        # 默认隐藏 (flavor=NAPCAT); _apply_flavor_visibility 会根据当前选中项切换
        sl_section_title.hide()
        sl_form_widget.hide()

        # 左栏底部也填充 stretch, 让左右高度不依赖各自内容累计高度
        # (已移除: stretch 会挤压表单内容, 导致标签显示不完整)

        # ---------------- 错误提示 ----------------
        self.error_label = CaptionLabel("", self)
        self.error_label.setObjectName("serverEditError")
        # 对话框为独立顶层窗口, 不吃 RemotePage 的 QSS, 故保留一条 inline 样式
        self.error_label.setStyleSheet("color: #C42B1C;")
        self.error_label.hide()
        self.viewLayout.addSpacing(4)
        self.viewLayout.addWidget(self.error_label)

    def _connect_signals(self) -> None:
        self.method_combo.currentIndexChanged.connect(self._on_method_changed)
        self.key_browse_btn.clicked.connect(self._on_browse_key)
        self.key_scan_btn.clicked.connect(self._on_scan_back)
        # W10b: flavor 切换 -> SL 分组显隐
        self.flavor_combo.currentIndexChanged.connect(self._on_flavor_changed)

    # ---------- 工具方法 ----------
    def _make_label(self, text: str) -> BodyLabel:
        """表单左侧字段名 (固定宽度 + 右对齐)."""
        label = BodyLabel(f"{text}:")
        label.setFixedWidth(self._LABEL_COLUMN_WIDTH)
        label.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        return label

    def _add_section(self, title: str, *, target: QVBoxLayout | None = None) -> QFormLayout:
        """在指定列 layout 末尾追加一个小标题 + QFormLayout 分组, 返回 form 供外部填充.

        Args:
            title: 分组标题
            target: 目标列 layout (W10b v2 两栏布局); 默认 None 时添加到 viewLayout

        Margin 约束:
        - 标题与上一组 form 之间只留 2px (标题本身自带 font ascent 足够作视觉间隔)
        - form 顶部 margin 为 0, 让首行与标题紧贴 (QFormLayout 行自带 verticalSpacing 做内部间隔)
        - 不再在 Python 中控制 LineEdit 高度, 交给 qfluentwidgets 自身 33px 默认
        """
        target_layout = target if target is not None else self.viewLayout
        section_title = StrongBodyLabel(title, self)
        section_title.setObjectName("serverEditSectionTitle")
        target_layout.addSpacing(2)
        target_layout.addWidget(section_title)

        form = QFormLayout()
        form.setContentsMargins(0, 0, 0, 0)
        form.setHorizontalSpacing(10)
        form.setVerticalSpacing(10)
        form.setLabelAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        form.setFieldGrowthPolicy(QFormLayout.FieldGrowthPolicy.AllNonFixedFieldsGrow)
        target_layout.addLayout(form)
        return form

    # ==================== 数据加载 ====================
    def _load_from_profile(self, profile: ServerProfile | None, existing_password: str | None) -> None:
        if profile is None:
            # 新建模式: 默认 NAPCAT, 应用 flavor 可见性
            self._apply_flavor_visibility(BackendFlavor.NAPCAT.value)
            return
        cred = profile.credentials
        self.name_edit.setText(profile.name)
        self.host_edit.setText(cred.host)
        self.port_edit.setText(str(cred.port))
        self.username_edit.setText(cred.username)

        # W10b: flavor + SL 字段回填; D8 决策: 编辑模式下 flavor 禁用
        flavor_value = profile.backend_flavor.value
        flavor_index = self.flavor_combo.findData(flavor_value)
        if flavor_index >= 0:
            self.flavor_combo.setCurrentIndex(flavor_index)
        self.flavor_combo.setEnabled(False)
        self.flavor_combo.setToolTip(
            "已保存档案的后端类型不可修改 (避免 NapCat/SnowLuma 字段语义错位).\n"
            "如需切换请删除档案后重新添加."
        )
        self._apply_flavor_visibility(flavor_value)

        if profile.snowluma_paths is not None:
            self.sl_base_dir_edit.setText(profile.snowluma_paths.base_dir)
        if profile.snowluma_webui_password_override:
            self.sl_webui_pwd_edit.setText(profile.snowluma_webui_password_override)

        method_index = self.method_combo.findData(cred.auth_method)
        if method_index >= 0:
            self.method_combo.setCurrentIndex(method_index)
        self._apply_method_visibility(cred.auth_method)

        if cred.private_key_path:
            # 已有路径: 若命中扫描候选则进入 combo 模式选中该项; 否则进入 manual 模式
            matched = -1
            for i in range(self.key_combo.count()):
                if self.key_combo.itemData(i) == cred.private_key_path:
                    matched = i
                    break
            if matched >= 0:
                self.key_combo.setCurrentIndex(matched)
                self._set_key_mode(manual=False)
            else:
                self.key_edit.setText(cred.private_key_path)
                self._set_key_mode(manual=True)
        if existing_password:
            self.pwd_edit.setText(existing_password)
        # P4 F5.2: 编辑模式下若 ServerManager 提示已记住密码, 默认勾选
        if self._existing_remember_password and self._credential_store.is_available():
            self.remember_check.setChecked(True)

        self.timeout_edit.setText(str(int(cred.connect_timeout)))
        self.cmd_timeout_edit.setText(str(int(cred.command_timeout)))

        policy_index = self.policy_combo.findData(cred.host_key_policy)
        if policy_index >= 0:
            self.policy_combo.setCurrentIndex(policy_index)

        self.notes_edit.setText(profile.notes)

    # ==================== 信号回调 ====================
    def _on_method_changed(self, index: int) -> None:
        self._apply_method_visibility(self.method_combo.itemData(index))

    def _on_flavor_changed(self, index: int) -> None:
        """W10b: backend_flavor 切换, 刷新 SL 分组可见性."""
        self._apply_flavor_visibility(self.flavor_combo.itemData(index))

    def _apply_flavor_visibility(self, flavor_value: str | None) -> None:
        """W10b: flavor=SNOWLUMA 时显示 SL 分组, 否则隐藏."""
        if self._sl_section_title is None or self._sl_form_widget is None:
            return
        is_sl = flavor_value == BackendFlavor.SNOWLUMA.value
        self._sl_section_title.setVisible(is_sl)
        self._sl_form_widget.setVisible(is_sl)

    def _apply_method_visibility(self, method: str) -> None:
        """认证方式切换时显示对应行, 隐藏另一组."""
        if self._auth_form is None:
            return
        is_key = method == "key"
        self._auth_form.setRowVisible(self._auth_row_key, is_key)
        self._auth_form.setRowVisible(self._auth_row_pwd, not is_key)
        # P4 F5.2: "记住密码" 仅在密码模式且 keyring 可用时可见
        keyring_available = self._credential_store.is_available()
        self._auth_form.setRowVisible(self._auth_row_remember, (not is_key) and keyring_available)
        # "自动配置 SSH 密钥" 仅在密码模式可见(无 keyring 也可用, 仅依赖本地文件系统)
        self._auth_form.setRowVisible(self._auth_row_auto_key, not is_key)

    def _set_key_mode(self, *, manual: bool) -> None:
        """切换私钥行的展示模式.

        - manual=False (combo): 显示扫描候选下拉 + 浏览按钮
        - manual=True  (manual): 显示路径输入框 + 浏览按钮 + 返回按钮 (仅有扫描候选时)

        无扫描结果时只能进入 manual 模式, 返回按钮永不显示.
        """
        self._key_manual_mode = manual
        self.key_combo.setVisible(not manual)
        self.key_edit.setVisible(manual)
        # 浏览按钮在两种模式下都可见; 返回按钮仅 manual 模式 + 有扫描候选时可见
        self.key_scan_btn.setVisible(manual and bool(self._scanned_keys))

    def _on_browse_key(self) -> None:
        """打开文件选择, 选定后切到 manual 模式并展示路径."""
        file_path, _ = QFileDialog.getOpenFileName(
            self,
            "选择 SSH 私钥",
            QDir.homePath(),
            "私钥文件 (*.pem id_rsa id_ed25519 *);;所有文件 (*)",
        )
        if file_path:
            self.key_edit.setText(file_path)
            self._set_key_mode(manual=True)

    def _on_scan_back(self) -> None:
        """从 manual 模式返回 combo 模式 (仅有扫描候选时该按钮才可见)."""
        self._set_key_mode(manual=False)

    # ==================== 表单校验与结果 ====================
    def validate(self) -> bool:  # noqa: D401 - 重写 MessageBoxBase 钩子
        """点击 yes 时被 MessageBoxBase 调用; 返回 False 则阻止关闭. """
        try:
            self._build_credentials_or_raise()
        except ValueError as exc:
            self.error_label.setText(str(exc))
            self.error_label.show()
            return False
        self.error_label.hide()
        return True

    def get_profile(self) -> ServerProfile:
        """构造并返回新的 [`ServerProfile`](src/core/remote/servers.py). 

        编辑模式下复用原 ``id`` / ``created_at`` / ``deployment_state`` / 探测缓存,
        新建模式下生成全新的 ``id``. 

        W10b: 根据 ``flavor_combo`` 选择构造对应 ``BackendFlavor`` + ``snowluma_paths``;
        D8 决策下编辑模式 flavor 不可变, 直接沿用 original.
        """
        credentials = self._build_credentials_or_raise()
        name = self.name_edit.text().strip() or credentials.host
        notes = self.notes_edit.text().strip()

        # W10b: 确定 flavor 与 SL 字段
        if self._original_profile is not None:
            # 编辑模式: flavor 禁用, 直接沿用原值
            flavor = self._original_profile.backend_flavor
        else:
            flavor_value = self.flavor_combo.currentData() or BackendFlavor.NAPCAT.value
            try:
                flavor = BackendFlavor(flavor_value)
            except ValueError:
                flavor = BackendFlavor.NAPCAT

        snowluma_paths: SnowLumaRemotePaths | None = None
        webui_pwd_override = ""
        if flavor == BackendFlavor.SNOWLUMA:
            base_dir = self.sl_base_dir_edit.text().strip() or "$HOME/snowluma-remote"
            try:
                snowluma_paths = SnowLumaRemotePaths.from_base(base_dir)
            except ValueError as exc:
                raise ValueError(f"SnowLuma 工作目录无效: {exc}") from exc
            webui_pwd_override = self.sl_webui_pwd_edit.text().strip()

        if self._original_profile is None:
            return ServerProfile.create(
                name=name,
                credentials=credentials,
                notes=notes,
                backend_flavor=flavor,
                snowluma_paths=snowluma_paths,
                snowluma_webui_password_override=webui_pwd_override,
            )

        original = self._original_profile
        # 编辑模式: 用户可能改了 SL 字段, 采用新值; 但 flavor 不变
        effective_sl_paths = snowluma_paths if flavor == BackendFlavor.SNOWLUMA else None
        effective_sl_pwd = webui_pwd_override if flavor == BackendFlavor.SNOWLUMA else ""
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
            backend_flavor=flavor,
            snowluma_paths=effective_sl_paths,
            snowluma_webui_password_override=effective_sl_pwd,
            snowluma_framework_version=original.snowluma_framework_version,
        )

    def get_password(self) -> str | None:
        """获取本次输入的密码; 仅密码认证模式下有效. """
        if self.method_combo.currentData() != "password":
            return None
        password = self.pwd_edit.text()
        return password or None

    def wants_auto_setup_key(self) -> bool:
        """用户是否勾选了"登录后自动配置 SSH 密钥".

        Returns:
            ``True`` 当且仅当: 当前是密码认证模式 + 勾选项已勾 + 密码非空.
            其他情况一律 ``False``, 调用方可放心地直接据此决定是否触发后台密钥下发任务.
        """
        if self.method_combo.currentData() != "password":
            return False
        if not self.pwd_edit.text():
            return False
        return bool(self.auto_key_check.isChecked())

    def wants_remember_password(self) -> bool:
        """P4 F5.2: 用户是否勾选了"记住密码".

        Returns:
            ``True`` 当且仅当: 当前是密码认证模式 + keyring 可用 + 勾选项已勾.
            非密码模式或 keyring 不可用时永远返回 ``False``, 调用方可放心
            把该值传给 [`ServerManager.add_server`](src/core/remote/server_manager.py)
            的 ``remember_password`` 参数.
        """
        if self.method_combo.currentData() != "password":
            return False
        if not self._credential_store.is_available():
            return False
        return bool(self.remember_check.isChecked())

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
        if auth_method == "key":
            if self._key_manual_mode:
                private_key_path = self.key_edit.text().strip() or None
            else:
                # combo 模式: 直接读取选中项 userData
                data = self.key_combo.currentData()
                private_key_path = data if isinstance(data, str) and data else None
        else:
            private_key_path = None

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
        credentials.validate()
        return credentials
