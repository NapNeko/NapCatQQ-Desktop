# -*- coding: utf-8 -*-
"""设置页当前配置导出对话框。"""

# 标准库导入
from pathlib import Path

# 第三方库导入
from qfluentwidgets import BodyLabel, CaptionLabel, CheckBox, MessageBoxBase, PushButton, SimpleCardWidget, TitleLabel, setFont
from PySide6.QtCore import Qt, QUrl
from PySide6.QtGui import QDesktopServices
from PySide6.QtWidgets import QGridLayout, QSizePolicy, QVBoxLayout, QWidget

# 项目内模块导入
from src.core.config.config_export import (
    ExportExecutionPlan,
    ExportExecutionResult,
    ExportScanResult,
    apply_config_export,
    scan_current_config_export,
)
from src.core.logging import LogSource, logger
from src.ui.components.drop_folder_widget import DropFolderWidget
from src.ui.components.info_bar import error_bar, success_bar, warning_bar
from src.ui.components.stacked_widget import TransparentStackedWidget


class _DialogSectionCard(SimpleCardWidget):
    """统一的 Fluent 分区卡片。"""

    def __init__(self, title: str, description: str | None = None, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)

        self.root_layout = QVBoxLayout(self)
        self.root_layout.setContentsMargins(18, 16, 18, 16)
        self.root_layout.setSpacing(8)

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
        self.body_widget.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)
        self.body_layout = QVBoxLayout(self.body_widget)
        self.body_layout.setContentsMargins(0, 0, 0, 0)
        self.body_layout.setSpacing(8)
        self.root_layout.addWidget(self.body_widget)


class _ConfigExportFolderPage(QWidget):
    """导出文件夹选择页。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)

        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 8, 0, 0)
        self.v_box_layout.setSpacing(14)

        self.drop_widget = DropFolderWidget(
            self,
            title_text=self.tr("拖拽保存文件夹到此处"),
            browse_text=self.tr("浏览文件夹"),
            dialog_title=self.tr("选择配置导出保存文件夹"),
        )
        self.drop_widget.setMinimumHeight(180)
        self.drop_widget.or_label.hide()

        self.preview_card = _DialogSectionCard(
            self.tr("导出预览"),
            self.tr("确认保存位置后，会自动生成带时间后缀的 ZIP 文件名。"),
            self,
        )
        self.folder_summary_widget = QWidget(self.preview_card.body_widget)
        self.folder_summary_widget.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)
        self.folder_summary_layout = QGridLayout(self.folder_summary_widget)
        self.folder_summary_layout.setContentsMargins(0, 0, 0, 0)
        self.folder_summary_layout.setHorizontalSpacing(16)
        self.folder_summary_layout.setVerticalSpacing(6)
        self.folder_summary_layout.setColumnStretch(1, 1)

        self.folder_title = BodyLabel(self.tr("保存文件夹"), self.folder_summary_widget)
        self.folder_value = CaptionLabel(self.tr("未选择"), self.folder_summary_widget)
        self.folder_value.setWordWrap(True)
        self.archive_preview_title = BodyLabel(self.tr("导出文件名"), self.folder_summary_widget)
        self.archive_preview_value = CaptionLabel(self.tr("将按时间自动生成"), self.folder_summary_widget)
        self.archive_preview_value.setWordWrap(True)
        setFont(self.folder_title, 15)
        setFont(self.folder_value, 14)
        setFont(self.archive_preview_title, 15)
        setFont(self.archive_preview_value, 14)
        self.folder_summary_layout.addWidget(self.folder_title, 0, 0, 1, 1, Qt.AlignmentFlag.AlignTop)
        self.folder_summary_layout.addWidget(self.folder_value, 0, 1)
        self.folder_summary_layout.addWidget(self.archive_preview_title, 1, 0, 1, 1, Qt.AlignmentFlag.AlignTop)
        self.folder_summary_layout.addWidget(self.archive_preview_value, 1, 1)
        self.preview_card.body_layout.addWidget(self.folder_summary_widget)

        self.v_box_layout.addWidget(self.drop_widget)
        self.v_box_layout.addWidget(self.preview_card)

    def set_selected_folder(self, folder_path: str) -> None:
        self.folder_value.setText(folder_path)

    def set_archive_name(self, archive_name: str) -> None:
        self.archive_preview_value.setText(archive_name)


class _ConfigExportOptionsPage(QWidget):
    """导出选项页。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)

        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 8, 0, 0)
        self.v_box_layout.setSpacing(14)

        self.summary_card = _DialogSectionCard(
            self.tr("输出信息"),
            self.tr("确认保存位置和即将生成的 ZIP 文件名。"),
            self,
        )
        self.summary_widget = QWidget(self.summary_card.body_widget)
        self.summary_layout = QGridLayout(self.summary_widget)
        self.summary_layout.setContentsMargins(0, 0, 0, 0)
        self.summary_layout.setHorizontalSpacing(16)
        self.summary_layout.setVerticalSpacing(10)
        self.summary_layout.setColumnStretch(1, 1)

        self.output_dir_title = BodyLabel(self.tr("保存文件夹"), self.summary_widget)
        self.output_dir_value = CaptionLabel(self.tr("未选择"), self.summary_widget)
        self.output_dir_value.setWordWrap(True)
        self.archive_title = BodyLabel(self.tr("导出文件"), self.summary_widget)
        self.archive_value = CaptionLabel(self.tr("等待选择"), self.summary_widget)
        self.archive_value.setWordWrap(True)

        for label in (self.output_dir_title, self.archive_title):
            setFont(label, 15)
        for label in (self.output_dir_value, self.archive_value):
            setFont(label, 14)

        self.summary_layout.addWidget(self.output_dir_title, 0, 0, 1, 1, Qt.AlignmentFlag.AlignTop)
        self.summary_layout.addWidget(self.output_dir_value, 0, 1)
        self.summary_layout.addWidget(self.archive_title, 1, 0, 1, 1, Qt.AlignmentFlag.AlignTop)
        self.summary_layout.addWidget(self.archive_value, 1, 1)
        self.summary_card.body_layout.addWidget(self.summary_widget)

        self.selection_card = _DialogSectionCard(
            self.tr("包含项"),
            self.tr("按需勾选要写入 ZIP 的配置项。"),
            self,
        )
        self.export_app_checkbox = CheckBox(self.tr("主配置 · config.json"), self.selection_card.body_widget)
        self.export_bot_checkbox = CheckBox(self.tr("Bot 配置 · bot.json"), self.selection_card.body_widget)
        setFont(self.export_app_checkbox, 14)
        setFont(self.export_bot_checkbox, 14)
        self.selection_card.body_layout.addWidget(self.export_app_checkbox)
        self.selection_card.body_layout.addWidget(self.export_bot_checkbox)

        self.v_box_layout.addWidget(self.summary_card)
        self.v_box_layout.addWidget(self.selection_card)

    def update_scan_result(self, scan_result: ExportScanResult) -> None:
        self.output_dir_value.setText(str(scan_result.output_dir))
        self.archive_value.setText(scan_result.archive_path.name)

        has_app_config = scan_result.app_config_path is not None
        has_bot_config = scan_result.bot_config_path is not None and scan_result.bot_count > 0

        self.export_app_checkbox.setEnabled(has_app_config)
        self.export_app_checkbox.setChecked(has_app_config)
        self.export_bot_checkbox.setEnabled(has_bot_config)
        self.export_bot_checkbox.setChecked(has_bot_config)


class _ConfigExportConfirmPage(QWidget):
    """导出确认与结果页。"""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)

        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 8, 0, 0)
        self.v_box_layout.setSpacing(14)

        self.summary_card = _DialogSectionCard(
            self.tr("导出确认"),
            self.tr("执行前再确认一次 ZIP 文件名、包含项与 Bot 数量。"),
            self,
        )

        self.summary_widget = QWidget(self.summary_card.body_widget)
        self.summary_layout = QGridLayout(self.summary_widget)
        self.summary_layout.setContentsMargins(0, 0, 0, 0)
        self.summary_layout.setHorizontalSpacing(16)
        self.summary_layout.setVerticalSpacing(10)
        self.summary_layout.setColumnStretch(1, 1)

        self.target_path_title = BodyLabel(self.tr("目标目录"), self.summary_widget)
        self.target_path_value = CaptionLabel(self.tr("未选择"), self.summary_widget)
        self.target_path_value.setWordWrap(True)
        self.file_list_title = BodyLabel(self.tr("将导出文件"), self.summary_widget)
        self.file_list_value = CaptionLabel(self.tr("等待选择"), self.summary_widget)
        self.file_list_value.setWordWrap(True)
        self.bot_count_title = BodyLabel(self.tr("Bot 数量"), self.summary_widget)
        self.bot_count_value = CaptionLabel("0", self.summary_widget)
        self.archive_title = BodyLabel(self.tr("ZIP 文件"), self.summary_widget)
        self.archive_value = CaptionLabel(self.tr("等待选择"), self.summary_widget)
        self.archive_value.setWordWrap(True)

        for label in (self.target_path_title, self.archive_title, self.file_list_title, self.bot_count_title):
            setFont(label, 15)
        for label in (self.target_path_value, self.archive_value, self.file_list_value, self.bot_count_value):
            setFont(label, 14)

        self.summary_layout.addWidget(self.target_path_title, 0, 0, 1, 1, Qt.AlignmentFlag.AlignTop)
        self.summary_layout.addWidget(self.target_path_value, 0, 1)
        self.summary_layout.addWidget(self.archive_title, 1, 0, 1, 1, Qt.AlignmentFlag.AlignTop)
        self.summary_layout.addWidget(self.archive_value, 1, 1)
        self.summary_layout.addWidget(self.file_list_title, 2, 0, 1, 1, Qt.AlignmentFlag.AlignTop)
        self.summary_layout.addWidget(self.file_list_value, 2, 1)
        self.summary_layout.addWidget(self.bot_count_title, 3, 0, 1, 1, Qt.AlignmentFlag.AlignTop)
        self.summary_layout.addWidget(self.bot_count_value, 3, 1)
        self.summary_card.body_layout.addWidget(self.summary_widget)

        self.v_box_layout.addWidget(self.summary_card)

    def update_preview(
        self,
        *,
        scan_result: ExportScanResult,
        export_app_config: bool,
        export_bot_config: bool,
    ) -> None:
        files: list[str] = []
        if export_app_config:
            files.append("config.json")
        if export_bot_config:
            files.append("bot.json")
        files.append("export_meta.json")

        self.summary_card.title_label.setText(self.tr("导出确认"))
        if self.summary_card.description_label is not None:
            self.summary_card.description_label.setText(self.tr("确认无误后即可开始导出。"))
        self.target_path_value.setText(str(scan_result.output_dir))
        self.archive_value.setText(scan_result.archive_path.name)
        self.file_list_value.setText("\n".join(files))
        self.bot_count_value.setText(str(scan_result.bot_count if export_bot_config else 0))

    def update_result(self, result: ExportExecutionResult) -> None:
        self.summary_card.title_label.setText(self.tr("导出完成"))
        if self.summary_card.description_label is not None:
            self.summary_card.description_label.setText(self.tr("ZIP 已写出到目标目录，可直接打开查看。"))
        self.target_path_value.setText(str(result.output_dir))
        self.archive_value.setText(result.archive_path.name)
        self.file_list_value.setText("\n".join(result.exported_files))
        self.bot_count_value.setText(str(result.exported_bot_count))


class ConfigExportDialog(MessageBoxBase):
    """当前配置导出对话框。"""

    def __init__(self, parent: QWidget) -> None:
        super().__init__(parent=parent)

        self._scan_result: ExportScanResult | None = None
        self._last_export_result: ExportExecutionResult | None = None
        self._export_completed = False

        self.title_label = TitleLabel(self.tr("导出当前配置"), self)
        self.content_label = CaptionLabel(self.tr("分三步完成：选择位置、确定内容、导出 ZIP。"), self)
        self.content_label.setWordWrap(True)
        setFont(self.content_label, 15)

        self.content_stack = TransparentStackedWidget(self)
        self.content_stack.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Preferred)
        self.folder_page = _ConfigExportFolderPage(self.content_stack)
        self.options_page = _ConfigExportOptionsPage(self.content_stack)
        self.confirm_page = _ConfigExportConfirmPage(self.content_stack)

        self.content_stack.addWidget(self.folder_page)
        self.content_stack.addWidget(self.options_page)
        self.content_stack.addWidget(self.confirm_page)
        self.content_stack.setCurrentWidget(self.folder_page)

        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addWidget(self.content_label)
        self.viewLayout.addWidget(self.content_stack)

        self.widget.setMinimumWidth(560)
        self.select_folder_button = PushButton(self.tr("重新选择位置"), self.widget)
        self.open_folder_button = PushButton(self.tr("打开保存文件夹"), self.widget)
        setFont(self.select_folder_button, 14)
        setFont(self.open_folder_button, 14)
        self.select_folder_button.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self.open_folder_button.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self.select_folder_button.hide()
        self.open_folder_button.hide()
        self.buttonLayout.insertWidget(self.buttonLayout.indexOf(self.cancelButton), self.select_folder_button, 1)
        self.buttonLayout.insertWidget(self.buttonLayout.indexOf(self.cancelButton), self.open_folder_button, 1)
        self.yesButton.setText(self.tr("选择文件夹"))
        self.cancelButton.setText(self.tr("取消"))
        self.yesButton.setEnabled(False)

        self.folder_page.drop_widget.folder_selected.connect(self._scan_target_folder)
        self.select_folder_button.clicked.connect(self._show_drop_page)
        self.open_folder_button.clicked.connect(self._open_target_folder)
        self.options_page.export_app_checkbox.toggled.connect(self._refresh_action_state)
        self.options_page.export_bot_checkbox.toggled.connect(self._refresh_action_state)

    def _choose_output_folder(self) -> None:
        folder = self.folder_page.drop_widget.browse_folder()
        if folder is not None:
            self.folder_page.set_selected_folder(str(folder))

    def _show_drop_page(self) -> None:
        self._export_completed = False
        self.content_stack.setCurrentWidget(self.folder_page)
        self.yesButton.setText(self.tr("选择文件夹"))
        self.yesButton.setEnabled(False)
        self.cancelButton.setText(self.tr("取消"))
        self.cancelButton.show()
        self.select_folder_button.hide()
        self.open_folder_button.hide()
        self._sync_dialog_height()

    def _show_options_page(self) -> None:
        self.content_stack.setCurrentWidget(self.options_page)
        self.yesButton.setText(self.tr("下一步"))
        self.cancelButton.setText(self.tr("返回文件夹选择"))
        self.cancelButton.show()
        self.select_folder_button.show()
        self.open_folder_button.hide()
        self._refresh_action_state()
        self._sync_dialog_height()

    def _show_confirm_page(self) -> None:
        self.content_stack.setCurrentWidget(self.confirm_page)
        self.yesButton.setText(self.tr("开始导出"))
        self.yesButton.setEnabled(True)
        self.cancelButton.setText(self.tr("返回导出选项"))
        self.cancelButton.show()
        self.select_folder_button.hide()
        self.open_folder_button.hide()
        self._sync_dialog_height()

    def _show_result_state(self) -> None:
        self.content_stack.setCurrentWidget(self.confirm_page)
        self.yesButton.setText(self.tr("完成"))
        self.yesButton.setEnabled(True)
        self.cancelButton.hide()
        self.select_folder_button.hide()
        self.open_folder_button.show()
        self._sync_dialog_height()

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
        button_height = max(self.buttonLayout.sizeHint().height(), 52)
        min_height = max(self.widget.minimumHeight(), 300)
        min_width = max(self.widget.minimumWidth(), 560)
        self.widget.setMinimumSize(min_width, min_height)
        target_height = max(min_height, min(current_height + header_height + button_height + 60, 620))
        self.widget.setFixedHeight(target_height)

    def _refresh_action_state(self) -> None:
        if self.content_stack.currentWidget() is not self.options_page or self._scan_result is None:
            return

        has_selection = self.options_page.export_app_checkbox.isChecked() or self.options_page.export_bot_checkbox.isChecked()
        self.yesButton.setEnabled(has_selection)

    def _build_export_plan(self) -> ExportExecutionPlan:
        if self._scan_result is None:
            raise ValueError("当前没有可用的导出扫描结果")

        return ExportExecutionPlan(
            scan_result=self._scan_result,
            export_app_config=self.options_page.export_app_checkbox.isChecked(),
            export_bot_config=self.options_page.export_bot_checkbox.isChecked(),
        )

    def _open_target_folder(self) -> None:
        target_path = None
        if self._last_export_result is not None:
            target_path = self._last_export_result.output_dir
        elif self._scan_result is not None:
            target_path = self._scan_result.output_dir

        if target_path is None:
            return

        QDesktopServices.openUrl(QUrl.fromLocalFile(str(target_path)))

    def _scan_target_folder(self, folder_path: object) -> None:
        try:
            self._scan_result = scan_current_config_export(Path(str(folder_path)))
        except Exception as error:
            logger.error(f"配置导出扫描失败: {type(error).__name__}: {error}", log_source=LogSource.UI)
            self._scan_result = None
            self._last_export_result = None
            self._export_completed = False
            self.yesButton.setEnabled(False)
            error_bar(self.tr(f"扫描失败: {type(error).__name__}: {error}"), parent=self)
            return

        self._last_export_result = None
        self._export_completed = False
        self.folder_page.set_selected_folder(str(self._scan_result.output_dir))
        self.folder_page.set_archive_name(self._scan_result.archive_path.name)
        self.options_page.update_scan_result(self._scan_result)
        self._show_options_page()

        if self._scan_result.warnings:
            warning_bar("\n".join(self._scan_result.warnings), title=self.tr("导出提示"), duration=10000, parent=self)

    def reject(self) -> None:
        if self.content_stack.currentWidget() is self.confirm_page and not self._export_completed:
            self._show_options_page()
            return

        if self.content_stack.currentWidget() is self.options_page:
            self._show_drop_page()
            return

        super().reject()

    def accept(self) -> None:
        if self._export_completed:
            super().accept()
            return

        if self.content_stack.currentWidget() is self.folder_page:
            self._choose_output_folder()
            return

        if self._scan_result is None:
            warning_bar(self.tr("请先选择导出目录"), parent=self)
            return

        if self.content_stack.currentWidget() is self.options_page:
            try:
                plan = self._build_export_plan()
            except ValueError as error:
                warning_bar(self.tr(str(error)), parent=self)
                return

            if not plan.export_app_config and not plan.export_bot_config:
                warning_bar(self.tr("当前没有可执行的导出项"), parent=self)
                return

            self.confirm_page.update_preview(
                scan_result=self._scan_result,
                export_app_config=plan.export_app_config,
                export_bot_config=plan.export_bot_config,
            )
            self._show_confirm_page()
            return

        try:
            result = apply_config_export(self._build_export_plan())
        except Exception as error:
            logger.error(f"配置导出失败: {type(error).__name__}: {error}", log_source=LogSource.UI)
            error_bar(self.tr(f"导出失败: {type(error).__name__}: {error}"), parent=self)
            return

        self._last_export_result = result
        self._export_completed = True
        self.confirm_page.update_result(result)
        self._show_result_state()

        success_bar(
            self.tr(
                f"已生成 {result.archive_path.name}"
                f"（主配置：{'是' if result.app_exported else '否'}，Bot 配置：{'是' if result.bot_exported else '否'}）"
            ),
            title=self.tr("当前配置已导出"),
            duration=7000,
            parent=self,
        )
        if result.warnings:
            warning_bar("\n".join(result.warnings), title=self.tr("导出提示"), duration=10000, parent=self)
