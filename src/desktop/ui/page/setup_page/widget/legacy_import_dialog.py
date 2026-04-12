# -*- coding: utf-8 -*-
"""设置页旧版配置导入对话框。"""

# 标准库导入
import platform
import shutil
import time
from dataclasses import dataclass
from pathlib import Path

# 第三方库导入
from creart import it
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    CheckBox,
    MessageBoxBase,
    PushButton,
    RadioButton,
    SimpleCardWidget,
    TransparentPushButton,
    TitleLabel,
    setFont,
)
from PySide6.QtCore import QObject, QRunnable, QThreadPool, Qt, Signal
from PySide6.QtWidgets import QButtonGroup, QGridLayout, QHBoxLayout, QSizePolicy, QVBoxLayout, QWidget

# 项目内模块导入
import src.desktop.core.config as app_config_module
from src.desktop.core.config import cfg
from src.desktop.core.config.legacy_import import (
    ImportConflictItem,
    ImportExecutionPlan,
    ImportScanResult,
    apply_legacy_config_import,
    scan_legacy_import_source,
)
from src.desktop.core.config.operate_config import read_config
from src.desktop.core.logging import LogSource, logger
from src.desktop.core.runtime.paths import PathFunc
from src.desktop.ui.components.drop_folder_widget import DropFolderWidget
from src.desktop.ui.components.info_bar import error_bar, success_bar, warning_bar
from src.desktop.ui.components.skeleton_widget import SkeletonShape, SkeletonWidget
from src.desktop.ui.components.stacked_widget import TransparentStackedWidget


@dataclass(frozen=True)
class _LegacyImportScanPayload:
    scan_result: ImportScanResult
    current_bot_count: int


class _DialogSectionCard(SimpleCardWidget):
    """统一的 Fluent 分区卡片。"""

    def __init__(self, title: str, description: str | None = None, parent: QWidget | None = None) -> None:
        super().__init__(parent)

        self.root_layout = QVBoxLayout(self)
        self.root_layout.setContentsMargins(20, 18, 20, 18)
        self.root_layout.setSpacing(10)

        self.title_label = BodyLabel(title, self)
        setFont(self.title_label, 15)
        self.root_layout.addWidget(self.title_label)

        self.description_label: CaptionLabel | None = None
        if description:
            self.description_label = CaptionLabel(description, self)
            self.description_label.setWordWrap(True)
            setFont(self.description_label, 13)
            self.root_layout.addWidget(self.description_label)

        self.body_widget = QWidget(self)
        self.body_layout = QVBoxLayout(self.body_widget)
        self.body_layout.setContentsMargins(0, 0, 0, 0)
        self.body_layout.setSpacing(10)
        self.root_layout.addWidget(self.body_widget)


class _LegacyImportScanTask(QObject, QRunnable):
    """旧版配置扫描后台任务。"""

    finished = Signal(int, object)
    failed = Signal(int, str, str)

    def __init__(self, folder: Path, token: int) -> None:
        QObject.__init__(self)
        QRunnable.__init__(self)
        self.folder = folder
        self.token = token

    def run(self) -> None:
        try:
            scan_result = scan_legacy_import_source(self.folder)
            current_bot_count = len(read_config())
        except Exception as error:
            self.failed.emit(self.token, type(error).__name__, str(error))
            return

        self.finished.emit(
            self.token,
            _LegacyImportScanPayload(
                scan_result=scan_result,
                current_bot_count=current_bot_count,
            ),
        )


class _LegacyImportDropPage(QWidget):
    """拖拽目录或 ZIP 选择页。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)

        self.drop_widget = DropFolderWidget(
            self,
            title_text=self.tr("拖拽导入源到此处"),
            browse_text=self.tr("浏览文件夹"),
            accepted_file_suffixes=(".zip",),
        )
        self.drop_widget.setMinimumHeight(180)
        self.select_zip_button = TransparentPushButton(self.tr("导入 ZIP 包"), self.drop_widget)
        setFont(self.drop_widget.browse_button, 17)
        setFont(self.select_zip_button, 17)

        self.drop_widget.or_label.hide()
        self.action_row = QWidget(self.drop_widget)
        self.action_row_layout = QHBoxLayout(self.action_row)
        self.action_row_layout.setContentsMargins(0, 0, 0, 0)
        self.action_row_layout.setSpacing(8)
        self.action_row_layout.setAlignment(Qt.AlignmentFlag.AlignHCenter)
        self.action_row_layout.addWidget(self.drop_widget.browse_button)
        self.action_row_layout.addWidget(self.select_zip_button)

        drop_layout = self.drop_widget.layout()
        if isinstance(drop_layout, QVBoxLayout):
            drop_layout.removeWidget(self.drop_widget.browse_button)
            drop_layout.insertWidget(2, self.action_row, 0, Qt.AlignmentFlag.AlignHCenter)

        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 8, 0, 0)
        self.v_box_layout.setSpacing(0)
        self.v_box_layout.addWidget(self.drop_widget, 1)


class _LegacyImportResultPage(QWidget):
    """识别结果页。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)

        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 8, 0, 0)
        self.v_box_layout.setSpacing(20)

        self.source_card = _DialogSectionCard(
            self.tr("来源概览"),
            self.tr("先确认导入源类型与位置，再决定是否继续。"),
            self,
        )
        self.source_grid = QGridLayout()
        self.source_grid.setContentsMargins(0, 0, 0, 0)
        self.source_grid.setHorizontalSpacing(16)
        self.source_grid.setVerticalSpacing(10)
        self.source_grid.setColumnStretch(1, 1)

        self.source_type_title = BodyLabel(self.tr("来源类型"), self.source_card.body_widget)
        self.source_type_value = CaptionLabel(self.tr("未选择"), self.source_card.body_widget)
        self.source_path_title = BodyLabel(self.tr("来源位置"), self.source_card.body_widget)
        self.source_path_value = CaptionLabel(self.tr("未选择"), self.source_card.body_widget)
        self.source_path_value.setWordWrap(True)

        self.summary_card = _DialogSectionCard(
            self.tr("识别到的内容"),
            self.tr("这里只展示可迁移的配置项，避免在执行前一次性看到过多信息。"),
            self,
        )
        self.summary_grid = QGridLayout()
        self.summary_grid.setContentsMargins(0, 0, 0, 0)
        self.summary_grid.setHorizontalSpacing(16)
        self.summary_grid.setVerticalSpacing(12)
        self.summary_grid.setColumnMinimumWidth(0, 132)
        self.summary_grid.setColumnStretch(1, 1)

        self.app_path_title = BodyLabel(self.tr("主配置"), self.summary_card.body_widget)
        self.app_path_value = CaptionLabel(self.tr("未识别"), self.summary_card.body_widget)
        self.app_path_value.setWordWrap(True)
        self.bot_path_title = BodyLabel(self.tr("Bot 配置"), self.summary_card.body_widget)
        self.bot_path_value = CaptionLabel(self.tr("未识别"), self.summary_card.body_widget)
        self.bot_path_value.setWordWrap(True)
        self.bot_count_title = BodyLabel(self.tr("Bot 数量"), self.summary_card.body_widget)
        self.bot_count_value = CaptionLabel("0", self.summary_card.body_widget)
        self.conflict_count_title = BodyLabel(self.tr("待确认冲突"), self.summary_card.body_widget)
        self.conflict_count_value = CaptionLabel("0", self.summary_card.body_widget)
        self.warning_title = BodyLabel(self.tr("风险提示"), self.summary_card.body_widget)
        self.warning_value = CaptionLabel(self.tr("等待扫描"), self.summary_card.body_widget)
        self.warning_value.setWordWrap(True)

        for label in (
            self.source_type_title,
            self.source_path_title,
            self.app_path_title,
            self.bot_path_title,
            self.bot_count_title,
            self.conflict_count_title,
            self.warning_title,
        ):
            setFont(label, 15)
        for label in (
            self.source_type_value,
            self.source_path_value,
            self.app_path_value,
            self.bot_path_value,
            self.bot_count_value,
            self.conflict_count_value,
            self.warning_value,
        ):
            setFont(label, 14)

        self.source_grid.addWidget(self.source_type_title, 0, 0, 1, 1, Qt.AlignmentFlag.AlignTop)
        self.source_grid.addWidget(self.source_type_value, 0, 1)
        self.source_grid.addWidget(self.source_path_title, 1, 0, 1, 1, Qt.AlignmentFlag.AlignTop)
        self.source_grid.addWidget(self.source_path_value, 1, 1)
        self.source_card.body_layout.addLayout(self.source_grid)

        self.summary_grid.addWidget(self.app_path_title, 0, 0, 1, 1, Qt.AlignmentFlag.AlignTop)
        self.summary_grid.addWidget(self.app_path_value, 0, 1)
        self.summary_grid.addWidget(self.bot_path_title, 1, 0, 1, 1, Qt.AlignmentFlag.AlignTop)
        self.summary_grid.addWidget(self.bot_path_value, 1, 1)
        self.summary_grid.addWidget(self.bot_count_title, 2, 0, 1, 1, Qt.AlignmentFlag.AlignTop)
        self.summary_grid.addWidget(self.bot_count_value, 2, 1)
        self.summary_grid.addWidget(self.conflict_count_title, 3, 0, 1, 1, Qt.AlignmentFlag.AlignTop)
        self.summary_grid.addWidget(self.conflict_count_value, 3, 1)
        self.summary_grid.addWidget(self.warning_title, 4, 0, 1, 1, Qt.AlignmentFlag.AlignTop)
        self.summary_grid.addWidget(self.warning_value, 4, 1)
        self.summary_card.body_layout.addLayout(self.summary_grid)

        self.next_step_card = _DialogSectionCard(
            self.tr("下一步"),
            self.tr("确认导入范围后才会开始写回；如果存在 Bot 冲突，再单独决定哪些 QQ 号需要覆盖。"),
            self,
        )
        self.next_step_content = CaptionLabel(
            self.tr("程序会先创建备份，再按当前版本格式写入配置。"),
            self.next_step_card.body_widget,
        )
        self.next_step_content.setWordWrap(True)
        setFont(self.next_step_content, 13)
        self.next_step_card.body_layout.addWidget(self.next_step_content)

        self.top_row_layout = QHBoxLayout()
        self.top_row_layout.setContentsMargins(0, 0, 0, 0)
        self.top_row_layout.setSpacing(16)
        self.top_row_layout.addWidget(self.source_card, 4)
        self.top_row_layout.addWidget(self.next_step_card, 3)

        self.summary_card.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)
        self.source_card.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)
        self.next_step_card.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)

        self.v_box_layout.addLayout(self.top_row_layout)
        self.v_box_layout.addWidget(self.summary_card)

    def _format_detected_path(self, path: Path | None, scan_result: ImportScanResult) -> str:
        if path is None:
            return self.tr("未识别")

        try:
            return str(path.relative_to(scan_result.scan_root_path))
        except ValueError:
            return str(path)

    def update_scan_result(self, scan_result: ImportScanResult) -> None:
        conflict_count = len(scan_result.conflicts)

        self.source_type_value.setText(self.tr("ZIP 导入包") if scan_result.source_kind == "zip" else self.tr("文件夹"))
        self.source_path_value.setText(str(scan_result.source_path))
        self.app_path_value.setText(self._format_detected_path(scan_result.app_config_path, scan_result))
        self.bot_path_value.setText(self._format_detected_path(scan_result.bot_config_path, scan_result))
        self.bot_count_value.setText(str(scan_result.imported_bot_count))
        self.conflict_count_value.setText(str(conflict_count))
        self.warning_value.setText(
            "\n".join(scan_result.warnings) if scan_result.warnings else self.tr("未发现明显问题")
        )
        self.conflict_count_title.setVisible(conflict_count > 0)
        self.conflict_count_value.setVisible(conflict_count > 0)


class _LegacyImportScanSkeletonPage(QWidget):
    """旧版配置扫描骨架屏。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)

        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 8, 0, 0)
        self.v_box_layout.setSpacing(14)

        self.title_label = BodyLabel(self.tr("正在分析导入源"), self)
        self.hint_label = CaptionLabel(
            self.tr("正在识别可导入的主配置与 Bot 配置，请稍候。"),
            self,
        )
        self.hint_label.setWordWrap(True)
        setFont(self.title_label, 17)
        setFont(self.hint_label, 14)

        self.step_card = SimpleCardWidget(self)
        self.step_layout = QVBoxLayout(self.step_card)
        self.step_layout.setContentsMargins(18, 16, 18, 16)
        self.step_layout.setSpacing(8)
        self.step_title = BodyLabel(self.tr("扫描阶段"), self.step_card)
        self.step_hint = CaptionLabel(
            self.tr("1. 检查目录或 ZIP 包内容\n" "2. 识别 config.json / bot.json\n" "3. 计算 Bot 数量与冲突项"),
            self.step_card,
        )
        self.step_hint.setWordWrap(True)
        setFont(self.step_title, 15)
        setFont(self.step_hint, 14)
        self.step_layout.addWidget(self.step_title)
        self.step_layout.addWidget(self.step_hint)

        self.canvas = SkeletonWidget(self._build_shapes, self, panel_margin=6, panel_radius=20)
        self.canvas.setMinimumHeight(220)
        self.canvas.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

        self.v_box_layout.addWidget(self.title_label)
        self.v_box_layout.addWidget(self.hint_label)
        self.v_box_layout.addWidget(self.step_card)
        self.v_box_layout.addWidget(self.canvas, 1)

    def _build_shapes(self, widget: QWidget) -> list[SkeletonShape]:
        panel_rect = widget.rect().adjusted(6, 6, -6, -6)
        if panel_rect.width() <= 0 or panel_rect.height() <= 0:
            return []
        x = panel_rect.x() + 10
        y = panel_rect.y() + 8
        width = panel_rect.width() - 20
        shapes: list[SkeletonShape] = []

        shapes.append(SkeletonShape(x, y, int(width * 0.18), 14, 0.88))
        y += 30
        shapes.append(SkeletonShape(x, y, int(width * 0.94), 88, 1.06, 18))
        y += 112

        shapes.append(SkeletonShape(x, y, int(width * 0.18), 14, 0.84))
        y += 22
        shapes.append(SkeletonShape(x, y, int(width * 0.92), 12, 0.92))
        y += 26
        shapes.append(SkeletonShape(x, y, int(width * 0.18), 14, 0.84))
        y += 22
        shapes.append(SkeletonShape(x, y, int(width * 0.88), 12, 0.92))
        y += 30

        shapes.append(SkeletonShape(x, y, int(width * 0.16), 14, 0.84))
        y += 24
        row_widths = (0.72, 0.48, 0.84)
        for ratio in row_widths:
            shapes.append(SkeletonShape(x, y, int(width * 0.22), 12, 0.86))
            shapes.append(SkeletonShape(x + int(width * 0.28), y, int(width * ratio), 12, 0.96))
            y += 22

        y += 10
        shapes.append(SkeletonShape(x, y, int(width * 0.94), 90, 1.04, 18))
        return shapes


class _LegacyImportOptionsPage(QWidget):
    """导入选项页。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)

        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 8, 0, 0)
        self.v_box_layout.setSpacing(20)

        self.app_mode_widget = _DialogSectionCard(
            self.tr("主配置"),
            self.tr("导入时会自动迁移到当前结构，适合一起恢复桌面端通用设置。"),
            self,
        )
        self.import_app_checkbox = CheckBox(self.tr("导入并迁移主配置"), self.app_mode_widget.body_widget)
        self.import_app_checkbox.setChecked(True)
        setFont(self.import_app_checkbox, 14)
        self.app_mode_widget.body_layout.addWidget(self.import_app_checkbox)
        self.app_mode_widget.hide()

        self.bot_mode_widget = _DialogSectionCard(
            self.tr("Bot 配置"),
            self.tr("如果本地已有 Bot，可选择整体覆盖，或仅把新的配置追加进来。"),
            self,
        )
        self.replace_radio = RadioButton(self.tr("覆盖全部"), self.bot_mode_widget.body_widget)
        self.append_radio = RadioButton(self.tr("仅追加新配置"), self.bot_mode_widget.body_widget)
        self.append_radio.setChecked(True)
        setFont(self.replace_radio, 14)
        setFont(self.append_radio, 14)
        self.bot_mode_group = QButtonGroup(self.bot_mode_widget)
        self.bot_mode_group.addButton(self.replace_radio)
        self.bot_mode_group.addButton(self.append_radio)
        self.bot_mode_widget.body_layout.addWidget(self.replace_radio)
        self.bot_mode_widget.body_layout.addWidget(self.append_radio)
        self.bot_mode_widget.hide()

        self.v_box_layout.addWidget(self.app_mode_widget)
        self.v_box_layout.addWidget(self.bot_mode_widget)

    def update_scan_context(self, *, has_app_config: bool, has_bot_mode: bool) -> None:
        self.app_mode_widget.setVisible(has_app_config)
        self.import_app_checkbox.setChecked(has_app_config)
        self.bot_mode_widget.setVisible(has_bot_mode)


class _LegacyImportConflictPage(QWidget):
    """冲突 Bot 选择页。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)

        self._conflict_checkboxes: dict[int, CheckBox] = {}

        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 8, 0, 0)
        self.v_box_layout.setSpacing(20)

        self.conflict_list_card = _DialogSectionCard(
            self.tr("冲突列表"),
            self.tr("导入配置与当前本地配置的 QQ 号重复时，会出现在这里。"),
            self,
        )
        self.conflict_count_label = CaptionLabel(self.tr("等待选择"), self.conflict_list_card.body_widget)
        setFont(self.conflict_count_label, 13)
        self.conflict_list_card.body_layout.addWidget(self.conflict_count_label)
        self.conflict_list_widget = QWidget(self.conflict_list_card.body_widget)
        self.conflict_list_layout = QVBoxLayout(self.conflict_list_widget)
        self.conflict_list_layout.setContentsMargins(0, 0, 0, 0)
        self.conflict_list_layout.setSpacing(10)
        self.conflict_list_layout.addStretch(1)
        self.conflict_list_card.body_layout.addWidget(self.conflict_list_widget)

        self.v_box_layout.addWidget(self.conflict_list_card)

    def set_conflicts(self, conflicts: tuple[ImportConflictItem, ...]) -> None:
        while self.conflict_list_layout.count() > 1:
            item = self.conflict_list_layout.takeAt(0)
            widget = item.widget()
            if widget is not None:
                widget.deleteLater()

        self._conflict_checkboxes.clear()
        self.conflict_count_label.setText(self.tr(f"共 {len(conflicts)} 项需要确认"))
        for conflict in conflicts:
            checkbox = CheckBox(
                self.tr(f"{conflict.qqid} · 当前「{conflict.current_name}」→ 导入「{conflict.imported_name}」"),
                self.conflict_list_widget,
            )
            setFont(checkbox, 14)
            self._conflict_checkboxes[conflict.qqid] = checkbox
            self.conflict_list_layout.insertWidget(self.conflict_list_layout.count() - 1, checkbox)

    def selected_qqids(self) -> frozenset[int]:
        return frozenset(qqid for qqid, checkbox in self._conflict_checkboxes.items() if checkbox.isChecked())

    def clear(self) -> None:
        self.set_conflicts(())


class LegacyImportDialog(MessageBoxBase):
    """旧版配置目录拖拽导入对话框。"""

    def __init__(self, parent: QWidget) -> None:
        super().__init__(parent=parent)

        self._scan_result: ImportScanResult | None = None
        self._current_bot_count = 0
        self._scan_token = 0
        self._active_scan_task: _LegacyImportScanTask | None = None

        self.title_label = TitleLabel(self.tr("导入旧版配置"), self)
        self.content_label = CaptionLabel(self.tr("先识别内容，再确认导入范围与冲突处理。"), self)
        self.content_label.setWordWrap(True)
        setFont(self.content_label, 15)

        self.content_stack = TransparentStackedWidget(self)
        self.content_stack.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Preferred)
        self.drop_page = _LegacyImportDropPage(self.content_stack)
        self.scan_page = _LegacyImportScanSkeletonPage(self.content_stack)
        self.result_page = _LegacyImportResultPage(self.content_stack)
        self.options_page = _LegacyImportOptionsPage(self.content_stack)
        self.conflict_page = _LegacyImportConflictPage(self.content_stack)

        self.content_stack.addWidget(self.drop_page)
        self.content_stack.addWidget(self.scan_page)
        self.content_stack.addWidget(self.result_page)
        self.content_stack.addWidget(self.options_page)
        self.content_stack.addWidget(self.conflict_page)
        self.content_stack.setCurrentWidget(self.drop_page)

        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addWidget(self.content_label)
        self.viewLayout.addWidget(self.content_stack)

        self.widget.setMinimumWidth(560)
        self.select_folder_button = PushButton(self.tr("重新选择来源"), self.widget)
        setFont(self.select_folder_button, 14)
        self.select_folder_button.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self.select_folder_button.hide()
        self.buttonLayout.insertWidget(self.buttonLayout.indexOf(self.cancelButton), self.select_folder_button, 1)
        self.yesButton.setText(self.tr("开始导入"))
        self.cancelButton.setText(self.tr("取消"))
        self.yesButton.setEnabled(False)

        self.drop_page.drop_widget.path_selected.connect(self._scan_folder)
        self.drop_page.select_zip_button.clicked.connect(self._select_zip_file)
        self.select_folder_button.clicked.connect(self._show_drop_page)
        self.options_page.replace_radio.toggled.connect(self._refresh_current_page_buttons)
        self.options_page.append_radio.toggled.connect(self._refresh_current_page_buttons)

    def _cleanup_scan_workspace(self) -> None:
        cleanup_path = self._scan_result.cleanup_path if self._scan_result is not None else None
        if cleanup_path is None or not cleanup_path.exists():
            return

        try:
            shutil.rmtree(cleanup_path, ignore_errors=True)
        except Exception as error:
            logger.warning(f"导入临时目录清理失败: {type(error).__name__}: {error}", log_source=LogSource.UI)

    def _select_zip_file(self) -> None:
        from PySide6.QtWidgets import QFileDialog

        zip_path, _ = QFileDialog.getOpenFileName(
            self,
            self.tr("选择旧版配置 ZIP 包"),
            "",
            self.tr("ZIP Files (*.zip)"),
        )
        if not zip_path:
            return

        self.drop_page.drop_widget.set_folder_path(Path(zip_path))

    def _show_drop_page(self) -> None:
        self._cleanup_scan_workspace()
        self._scan_result = None
        self.content_stack.setCurrentWidget(self.drop_page)
        self.yesButton.setEnabled(False)
        self.yesButton.setText(self.tr("开始导入"))
        self.cancelButton.setText(self.tr("取消"))
        self.cancelButton.show()
        self.select_folder_button.hide()
        self._sync_dialog_height()

    def _show_result_page(self) -> None:
        self.content_stack.setCurrentWidget(self.result_page)
        self.yesButton.setText(self.tr("下一步") if self._has_configurable_options() else self.tr("开始导入"))
        self.cancelButton.hide()
        self.select_folder_button.show()
        self._sync_dialog_height()

    def _show_scan_page(self) -> None:
        self.content_stack.setCurrentWidget(self.scan_page)
        self.yesButton.setEnabled(False)
        self.yesButton.setText(self.tr("开始导入"))
        self.cancelButton.setText(self.tr("取消"))
        self.cancelButton.show()
        self.select_folder_button.hide()
        self._sync_dialog_height()

    def _show_options_page(self) -> None:
        self.content_stack.setCurrentWidget(self.options_page)
        self.yesButton.setText(self.tr("下一步") if self._requires_conflict_selection() else self.tr("开始导入"))
        self.cancelButton.setText(self.tr("返回结果"))
        self.cancelButton.show()
        self.select_folder_button.hide()
        self._sync_dialog_height()

    def _show_conflict_page(self) -> None:
        self.content_stack.setCurrentWidget(self.conflict_page)
        self.yesButton.setText(self.tr("开始导入"))
        self.cancelButton.setText(self.tr("返回导入选项"))
        self.cancelButton.show()
        self.select_folder_button.hide()
        self._sync_dialog_height()

    def _refresh_current_page_buttons(self) -> None:
        if self.content_stack.currentWidget() is self.options_page:
            self._show_options_page()

    def _sync_dialog_height(self) -> None:
        self.viewLayout.activate()
        current_widget = self.content_stack.currentWidget()
        current_height = 0
        if current_widget is not None:
            current_widget.ensurePolished()
            current_widget.updateGeometry()
            if (layout := current_widget.layout()) is not None:
                layout.activate()
                available_width = max(current_widget.width(), self.content_stack.width(), self.widget.width() - 72, 480)
                if layout.hasHeightForWidth():
                    current_height = layout.totalHeightForWidth(available_width)
                else:
                    current_height = max(layout.sizeHint().height(), current_widget.minimumSizeHint().height())
            else:
                current_height = max(current_widget.sizeHint().height(), current_widget.minimumSizeHint().height())
        header_height = self.title_label.sizeHint().height() + self.content_label.sizeHint().height()
        button_height = max(self.buttonLayout.sizeHint().height(), 56)
        min_height = max(self.widget.minimumHeight(), 340)
        min_width = max(self.widget.minimumWidth(), 560)
        self.widget.setMinimumSize(min_width, min_height)
        target_height = max(min_height, min(current_height + header_height + button_height + 56, 680))
        self.widget.setFixedHeight(target_height)

    def _has_configurable_options(self) -> bool:
        if self._scan_result is None:
            return False

        has_app_option = self._scan_result.app_config_path is not None
        has_bot_option = self._scan_result.bot_config_path is not None and self._current_bot_count > 0
        return has_app_option or has_bot_option

    def _requires_conflict_selection(self) -> bool:
        if self._scan_result is None or self._scan_result.bot_config_path is None:
            return False

        if self._current_bot_count <= 0 or self.options_page.replace_radio.isChecked():
            return False

        return bool(self._scan_result.conflicts)

    def _scan_folder(self, folder_path: object) -> None:
        folder = Path(str(folder_path))
        self._scan_token += 1
        self._cleanup_scan_workspace()
        self._scan_result = None
        self._show_scan_page()

        task = _LegacyImportScanTask(folder, self._scan_token)
        self._active_scan_task = task
        task.finished.connect(self._on_scan_finished)
        task.failed.connect(self._on_scan_failed)
        QThreadPool.globalInstance().start(task)

    def _on_scan_finished(self, scan_token: int, payload: _LegacyImportScanPayload) -> None:
        if scan_token != self._scan_token:
            return

        self._active_scan_task = None
        self._scan_result = payload.scan_result
        self._current_bot_count = payload.current_bot_count

        has_app_config = self._scan_result.app_config_path is not None
        has_bot_mode = self._scan_result.bot_config_path is not None and self._current_bot_count > 0
        has_import_target = has_app_config or self._scan_result.bot_config_path is not None

        self.result_page.update_scan_result(self._scan_result)
        self.options_page.update_scan_context(has_app_config=has_app_config, has_bot_mode=has_bot_mode)
        self.conflict_page.clear()
        self.yesButton.setEnabled(has_import_target)
        self._show_result_page()

        if self._scan_result.warnings:
            warning_bar(
                "\n".join(self._scan_result.warnings),
                title=self.tr("扫描结果"),
                duration=12000,
                parent=self,
            )

    def _on_scan_failed(self, scan_token: int, error_type: str, error_message: str) -> None:
        if scan_token != self._scan_token:
            return

        self._active_scan_task = None
        logger.error(f"旧版配置扫描失败: {error_type}: {error_message}", log_source=LogSource.UI)
        self._scan_result = None
        self._current_bot_count = 0
        self.conflict_page.clear()
        self.yesButton.setEnabled(False)
        self.options_page.update_scan_context(has_app_config=False, has_bot_mode=False)
        self._show_drop_page()
        error_bar(self.tr(f"扫描失败: {error_type}: {error_message}"), parent=self)

    def _resolve_bot_import_plan(self) -> tuple[str, frozenset[int]]:
        if self._scan_result is None or self._scan_result.bot_config_path is None:
            return "skip", frozenset()

        if self._current_bot_count <= 0:
            return "replace", frozenset()

        if self.options_page.replace_radio.isChecked():
            return "replace", frozenset()

        return "append", self.conflict_page.selected_qqids()

    def _reload_runtime_config(self) -> None:
        cfg.load(it(PathFunc).config_path)
        cfg.set(cfg.start_time, time.time(), True)
        cfg.set(cfg.napcat_desktop_version, app_config_module.__version__, True)
        cfg.set(cfg.system_type, platform.system(), True)
        cfg.set(cfg.platform_type, platform.machine(), True)

    def _refresh_bot_list(self) -> None:
        try:
            from src.desktop.ui.page.bot_page import BotPage

            it(BotPage).bot_list_page.update_bot_list()
        except Exception as error:
            logger.warning(f"Bot 列表刷新失败: {type(error).__name__}: {error}", log_source=LogSource.UI)

    def reject(self) -> None:
        if self.content_stack.currentWidget() is self.scan_page:
            self._scan_token += 1
            self._active_scan_task = None
            super().reject()
            return

        if self.content_stack.currentWidget() is self.conflict_page:
            self._show_options_page()
            return

        if self.content_stack.currentWidget() is self.options_page:
            self._show_result_page()
            return

        super().reject()

    def done(self, result: int) -> None:
        self._scan_token += 1
        self._active_scan_task = None
        self._cleanup_scan_workspace()
        super().done(result)

    def accept(self) -> None:
        if self._scan_result is None:
            warning_bar(self.tr("请先拖入或选择旧版配置目录"), parent=self)
            return

        if self.content_stack.currentWidget() is self.result_page and self._has_configurable_options():
            self._show_options_page()
            return

        if self.content_stack.currentWidget() is self.options_page and self._requires_conflict_selection():
            self.conflict_page.set_conflicts(self._scan_result.conflicts)
            self._show_conflict_page()
            return

        import_app_config = (
            self._scan_result.app_config_path is not None and self.options_page.import_app_checkbox.isChecked()
        )
        bot_import_mode, overwrite_conflict_qqids = self._resolve_bot_import_plan()

        if not import_app_config and bot_import_mode == "skip":
            warning_bar(self.tr("当前没有可执行的导入项"), parent=self)
            return

        try:
            result = apply_legacy_config_import(
                ImportExecutionPlan(
                    scan_result=self._scan_result,
                    import_app_config=import_app_config,
                    bot_import_mode=bot_import_mode,
                    overwrite_conflict_qqids=overwrite_conflict_qqids,
                )
            )
        except Exception as error:
            logger.error(f"旧版配置导入失败: {type(error).__name__}: {error}", log_source=LogSource.UI)
            error_bar(self.tr(f"导入失败: {type(error).__name__}: {error}"), parent=self)
            return

        if result.app_imported:
            self._reload_runtime_config()
            warning_bar(
                self.tr("主配置已导入；若涉及重启项，请按提示重启应用。"),
                title=self.tr("主配置已导入"),
                duration=8000,
                parent=self,
            )

        if result.bot_imported:
            self._refresh_bot_list()

        success_bar(
            self.tr(
                f"Bot 导入 {result.imported_bot_count} 个，覆盖 {result.replaced_bot_count} 个，"
                f"新增 {result.appended_bot_count} 个，跳过 {result.skipped_bot_count} 个"
            ),
            title=self.tr("旧版配置已导入"),
            duration=7000,
            parent=self,
        )
        if result.warnings:
            warning_bar("\n".join(result.warnings), title=self.tr("导入提示"), duration=10000, parent=self)

        success_bar(
            self.tr(f"备份位置：{result.backup_dir}"),
            title=self.tr("已创建导入备份"),
            duration=6000,
            parent=self,
        )
        super().accept()
