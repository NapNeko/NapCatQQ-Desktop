# -*- coding: utf-8 -*-
"""[`DeploymentConsoleDialog`](src/ui/page/remote_page/deployment_console.py): 部署控制台对话框. 

基于 [`MessageBoxBase`](https://qfluentwidgets.com/) 实现, 与项目内其他对话框风格一致;
订阅 [`ServerManager`](src/core/remote/server_manager.py) 的:

- ``deployment_log(server_id, line)`` - 实时回显每行 ``\\n`` 终止的最终 stdout (含合并的 stderr)
- ``deployment_log_progress(server_id, line)`` - ``\\r`` 终止的瞬时刷新行 (dnf/apt/curl 进度条),
  UI 端以 "原地覆盖上一行" 的方式渲染, 模拟真实终端的 carriage-return 行为,
  避免 ``dnf`` 安装时上千次进度更新把整个控制台刷屏
- ``deployment_progress(server_id, message, percent)`` - 顶部进度条 + 阶段标题
- ``deployment_finished(server_id, ok, message)`` - 终结提示 + 关闭按钮启用

设计要点:
- 模态: 使用 [`MaskDialogBase`](https://qfluentwidgets.com/) 提供的遮罩层, 与项目风格一致
- 按 ``server_id`` 过滤信号, 互不干扰
- 终端复用项目内组件 [`CodeExibit`](src/ui/components/code_editor/exhibit.py)
  + [`LogHighlighter`](src/ui/components/code_editor/highlight.py),
  自动识别 ``[INFO]`` / ``[WARN]`` / ``[ERROR]`` / ``[SUCCESS]`` / ``[PROGRESS]`` 与时间戳/URL/路径
- 部署期间禁用关闭按钮, 防止误关
- 失败时控制台保持显示, 让用户能看到错误细节
"""

from __future__ import annotations

from creart import it
from PySide6.QtGui import QTextCursor
from PySide6.QtWidgets import (
    QApplication,
    QHBoxLayout,
    QPlainTextEdit,
    QSizePolicy,
    QWidget,
)
from qfluentwidgets import (
    CaptionLabel,
    MessageBoxBase,
    ProgressBar,
    PushButton,
    SubtitleLabel,
    FluentIcon as FI,
)

from src.core.remote import ServerManager
from src.ui.components.code_editor import CodeExibit, LogHighlighter


# 控制台终端的最大行数, 超过会自动从顶部裁剪, 防止内存膨胀
_MAX_LOG_LINES = 5000


class DeploymentConsoleDialog(MessageBoxBase):
    """部署控制台对话框. 

    用法::

        console = DeploymentConsoleDialog(server_id, server_name, parent=window)
        console.show()  # 非阻塞显示, 让 DeploymentRunner 在后台跑
    """

    def __init__(
        self,
        server_id: str,
        server_name: str,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent=parent)
        self._server_id = server_id
        self._server_name = server_name
        self._is_finished = False
        # 最近一次写入终端的块是否为 "\r 终止的瞬时刷新行"; 若是, 下一条日志
        # (无论是新的瞬时行还是最终行) 都会原地覆盖该块, 模拟真实终端 carriage-return 行为. 
        self._last_line_transient = False

        self._setup_ui()
        self._connect_signals()

    # ---------- UI ----------
    def _setup_ui(self) -> None:
        # 标题
        self.title_label = SubtitleLabel(f"正在部署: {self._server_name}", self)
        self.viewLayout.addWidget(self.title_label)

        self.stage_label = CaptionLabel("准备启动部署...", self)
        self.stage_label.setStyleSheet("color: #8a8a8a;")
        self.viewLayout.addWidget(self.stage_label)

        # 进度条
        self.progress_bar = ProgressBar(self)
        self.progress_bar.setRange(0, 100)
        self.progress_bar.setValue(0)
        self.viewLayout.addWidget(self.progress_bar)

        # 终端区: 复用项目组件 CodeExibit + LogHighlighter
        # CodeExibit 已经处理好主题/字体/平滑滚动等细节, 配合 LogHighlighter 自动着色
        self.terminal = CodeExibit(self)
        self.terminal.set_font_size(11)
        self.terminal.setMaximumBlockCount(_MAX_LOG_LINES)
        self.terminal.setMinimumHeight(360)
        self.terminal.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        # 显式开启按控件宽度自动换行, 长 URL/路径/curl 进度条不再溢出右侧
        self.terminal.setLineWrapMode(QPlainTextEdit.LineWrapMode.WidgetWidth)
        # LogHighlighter 内置识别 [INFO]/[WARN]/[ERROR]/[SUCCESS]/时间戳/URL/路径 等
        self._highlighter = LogHighlighter(self.terminal.document())
        self.viewLayout.addWidget(self.terminal, 1)

        # 控制台辅助按钮 (复制 / 清空) 放在 viewLayout 中, 与 yesButton/cancelButton 区分
        helper_row = QHBoxLayout()
        helper_row.setSpacing(8)
        self.copy_btn = PushButton(FI.COPY, "复制全部", self)
        self.copy_btn.clicked.connect(self._on_copy)
        self.clear_btn = PushButton(FI.DELETE, "清空", self)
        self.clear_btn.clicked.connect(self._on_clear)
        helper_row.addWidget(self.copy_btn)
        helper_row.addWidget(self.clear_btn)
        helper_row.addStretch()
        self.viewLayout.addLayout(helper_row)

        # MessageBoxBase 自带的 yesButton/cancelButton:
        # - yesButton: 部署期间作为 "取消部署" 按钮 (调 ServerManager.request_cancel)
        # - cancelButton: 部署期间禁用 "关闭"; 完成后才启用
        self.yesButton.setText("取消部署")
        self.yesButton.clicked.disconnect()  # 解除 MessageBoxBase 默认 accept 绑定
        self.yesButton.clicked.connect(self._on_cancel_deployment)
        self.cancelButton.setText("关闭")
        self.cancelButton.setEnabled(False)

        # 设置最小尺寸
        self.widget.setMinimumSize(820, 580)

    def _connect_signals(self) -> None:
        manager = it(ServerManager)
        manager.deployment_progress.connect(self._on_progress)
        manager.deployment_log.connect(self._on_log_line)
        manager.deployment_log_progress.connect(self._on_log_progress)
        manager.deployment_finished.connect(self._on_finished)

    # ---------- 信号处理 ----------
    def _on_progress(self, server_id: str, message: str, percent: int) -> None:
        if server_id != self._server_id:
            return
        self.progress_bar.setValue(max(0, min(100, percent)))
        self.stage_label.setText(f"{percent}% — {message}" if message else f"{percent}%")

    def _on_log_line(self, server_id: str, line: str) -> None:
        if server_id != self._server_id:
            return
        self._write_line(line, transient=False)

    def _on_log_progress(self, server_id: str, line: str) -> None:
        """``\\r`` 终止的瞬时刷新行 (dnf/apt/curl 进度条更新) 处理.

        UI 端以 "原地覆盖上一行" 的方式渲染, 模拟真实终端 carriage-return 行为,
        避免同一条 ``Installing`` 行因进度条帧刷新被重复堆成上千条.
        """
        if server_id != self._server_id:
            return
        self._write_line(line, transient=True)

    def _on_finished(self, server_id: str, ok: bool, message: str) -> None:
        if server_id != self._server_id:
            return
        self._is_finished = True
        # 完成后: 隐藏取消部署按钮, 启用关闭按钮
        self.yesButton.hide()
        self.cancelButton.setEnabled(True)
        if ok:
            self.title_label.setText(f"✅ 部署完成: {self._server_name}")
            self.stage_label.setText(message)
            self.stage_label.setStyleSheet("color: #107c10; font-weight: 600;")
            self._write_line("", transient=False)
            # 加 [SUCCESS] 让 LogHighlighter 自动着色
            self._write_line(f"[SUCCESS] {message}", transient=False)
        else:
            self.title_label.setText(f"❌ 部署失败: {self._server_name}")
            self.stage_label.setText(message)
            self.stage_label.setStyleSheet("color: #d83b01; font-weight: 600;")
            self._write_line("", transient=False)
            self._write_line(f"[ERROR] 部署失败: {message}", transient=False)

    # ---------- 终端渲染 ----------
    def _write_line(self, line: str, *, transient: bool) -> None:
        """向终端写入一行, 遵循 carriage-return 语义.

        - 若上一条写入是瞬时行 (``\\r`` 终止), 无论本次是瞬时还是最终行, 都 *覆盖*
          最后一个块的内容 (模拟真实终端进度条的原地刷新 / 提交). 
        - 否则追加新块. 
        - 写入后刷新 ``_last_line_transient`` 状态. 

        纯文本写入 (``insertText``/``appendPlainText``), 不走 HTML, 避免空字符串变成段落
        分隔产生额外空行; 后续由 :class:`LogHighlighter` 对整块文本着色.
        """
        if self._last_line_transient:
            # 覆盖最后一个块: 移动到末尾 -> 选中当前块全部文本 -> 替换
            cursor = self.terminal.textCursor()
            cursor.movePosition(QTextCursor.MoveOperation.End)
            cursor.movePosition(
                QTextCursor.MoveOperation.StartOfBlock,
                QTextCursor.MoveMode.MoveAnchor,
            )
            cursor.movePosition(
                QTextCursor.MoveOperation.EndOfBlock,
                QTextCursor.MoveMode.KeepAnchor,
            )
            cursor.removeSelectedText()
            cursor.insertText(line)
        else:
            self.terminal.appendPlainText(line)
        self.terminal.moveCursor(QTextCursor.MoveOperation.End)
        self._last_line_transient = transient

    # ---------- 按钮回调 ----------
    def _on_cancel_deployment(self) -> None:
        """"取消部署" 按钮: 调 ServerManager.request_cancel 发出协作式取消信号.

        为防误点增加 :class:`AskBox` 二次确认. 点击后:

        - 按钮文案变为 "取消中..." 并禁用 (防重复点)
        - 调 :meth:`ServerManager.request_cancel` set Event; 后台线程在下一个埋点处招
          :class:`RemoteDeploymentCancelledError` 并走 ``deployment_finished(False)`` 分支
        - 对话框**不会立即关闭**, 仍由 ``_on_finished`` 接手 (让用户看到 [INFO] 取消提示)
        """
        # 延迟 import: 避免该对话框在最早期加载时误拉 ServerManager
        from creart import it as _it

        from src.core.remote import ServerManager as _ServerManager
        from src.ui.components.message_box import AskBox

        if self._is_finished:
            return

        box = AskBox(
            "取消部署",
            "确认要取消远端部署任务吗?\n\n"
            "已上传到远端的文件会保留, 下次部署会复用. "
            "当前正在执行的 SSH 命令 / SFTP 上传会等当前调用返回后才退出.",
            self,
        )
        box.yesButton.setText("确认取消")
        box.cancelButton.setText("继续部署")
        box.cancelButton.setDefault(True)
        box.yesButton.setDefault(False)
        if not box.exec():
            return

        self.yesButton.setText("取消中...")
        self.yesButton.setEnabled(False)
        self.stage_label.setText("正在取消部署, 请等待当前步骤退出...")
        try:
            _it(_ServerManager).request_cancel(self._server_id)
        except Exception:  # noqa: BLE001 - 取消调用本身不该报错中断对话框
            pass

    def _on_copy(self) -> None:
        text = self.terminal.toPlainText()
        if not text:
            return
        clipboard = QApplication.clipboard()
        clipboard.setText(text)

    def _on_clear(self) -> None:
        self.terminal.clear()
        # 清空后下一行应以"追加"而非"覆盖"写入, 复位瞬时行标记
        self._last_line_transient = False

    # ---------- 生命周期 ----------
    def reject(self) -> None:  # noqa: D401 - 重写 cancelButton 触发的关闭
        if not self._is_finished:
            # 部署期间不允许关闭
            return
        super().reject()

    def closeEvent(self, event) -> None:  # noqa: N802 - Qt 重写
        if not self._is_finished:
            event.ignore()
            return
        super().closeEvent(event)
