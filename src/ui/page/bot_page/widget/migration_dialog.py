# -*- coding: utf-8 -*-
"""[`MigrationDialog`](src/ui/page/bot_page/widget/migration_dialog.py): Bot 运行位置迁移确认对话框 (P3.W3.B).

在 [`BotConfigPage.slot_save_config_button`](src/ui/page/bot_page/sub_page/bot_config.py)
检测到 ``runtime_target`` 变化时弹出, 让用户:

- 确认搬迁的源端与目标端
- 选择是否搬运 NapCat 持久数据 (P3 仅迁配置, future flag)
- 看到风险提示 (Bot 会被停止 / 不会自动启动 / 失败会保留源端)
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from PySide6.QtWidgets import QWidget
from qfluentwidgets import BodyLabel, CaptionLabel, CheckBox, MessageBoxBase, TitleLabel

if TYPE_CHECKING:
    pass


class MigrationDialog(MessageBoxBase):
    """Bot 运行位置迁移二次确认.

    返回值:
        ``exec()`` 返回 1 时, ``get_move_persistent_data()`` 表示用户是否勾选
        了"搬运持久数据"; 返回 0 表示用户取消.
    """

    def __init__(
        self,
        qq_id: str,
        source_label: str,
        dest_label: str,
        parent: QWidget,
    ) -> None:
        super().__init__(parent=parent)

        self.title_label = TitleLabel(self.tr("确认迁移 Bot 运行位置"), self)

        summary = (
            f"Bot QQ: {qq_id}\n"
            f"源端 → 目标:  {source_label}  →  {dest_label}"
        )
        self.summary_label = BodyLabel(summary, self)

        self.detail_label = BodyLabel(
            self.tr(
                "迁移将执行:\n"
                "  1. 停止源端正在运行的 Bot\n"
                "  2. 把 NapCat 配置文件 (onebot11/napcat JSON) 复制到目标端\n"
                "  3. 清理源端原有配置\n"
                "  4. 完成后**不会自动启动**目标端 Bot, 由你决定何时启动\n\n"
                "如果迁移失败, 源端配置保留, 目标端会留下半成品 (可重试)."
            ),
            self,
        )
        self.detail_label.setWordWrap(True)

        self.move_persistent_data_checkbox = CheckBox(
            self.tr("同时搬运 NapCat 持久数据 (账号缓存 / 数据库)"), self
        )
        self.move_persistent_data_checkbox.setChecked(False)

        self.hint_label = CaptionLabel(
            self.tr(
                "提示: 持久数据迁移在 P4 阶段评估; "
                "当前版本只迁移 NapCat 配置, 持久数据如有需要请手动备份."
            ),
            self,
        )
        self.hint_label.setStyleSheet("color: #8a8a8a;")
        self.hint_label.setWordWrap(True)

        self.widget.setMinimumSize(520, 360)
        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addWidget(self.summary_label)
        self.viewLayout.addWidget(self.detail_label)
        self.viewLayout.addWidget(self.move_persistent_data_checkbox)
        self.viewLayout.addWidget(self.hint_label)

        self.yesButton.setText(self.tr("确认迁移"))
        self.cancelButton.setText(self.tr("取消"))
        # 取消作为视觉默认, 防止"回车"误触发破坏性操作
        self.cancelButton.setDefault(True)
        self.yesButton.setDefault(False)

    def get_move_persistent_data(self) -> bool:
        """返回用户对持久数据迁移的选择 (P3 当前不实际生效, 留作 future flag)."""
        return self.move_persistent_data_checkbox.isChecked()
