# -*- coding: utf-8 -*-
"""远端服务器维护对话框集合.

- [`MaintenanceDialog`]: 维护操作选择器, 集中展示版本信息 + 4 个维护动作 (刷新版本 /
  强制更新 NapCat / 强制重装 LinuxQQ / 回滚部署).
- [`RollbackConfirmBox`]: 回滚二次确认 (危险动作), 由 [`MaintenanceDialog`] 选择
  "回滚" 后再次弹出.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from PySide6.QtCore import Signal
from PySide6.QtWidgets import QFormLayout, QWidget
from qfluentwidgets import (
    BodyLabel,
    CaptionLabel,
    CheckBox,
    FluentIcon as FI,
    MessageBoxBase,
    PushSettingCard,
    StrongBodyLabel,
    TitleLabel,
)

from src.core.remote import DeploymentState, ServerProfile
from src.core.remote.servers import BackendFlavor

if TYPE_CHECKING:
    pass


_DEPLOYMENT_STATE_TEXT: dict[DeploymentState, str] = {
    DeploymentState.UNDEPLOYED: "未部署",
    DeploymentState.DEPLOYING: "部署中",
    DeploymentState.DEPLOYED: "已部署",
    DeploymentState.FAILED: "部署失败",
}


class MaintenanceDialog(MessageBoxBase):
    """远端服务器维护操作选择对话框 (P3.W2.A 升级版).

    UX 要点:
    - 顶部以表单 (QFormLayout) 列出当前服务器状态, NapCat / LinuxQQ 版本
    - 中部使用 [`PushSettingCard`] 列出 4 个维护动作, 每项带说明文案
    - 危险操作 (回滚) 通过文案 + 红色按钮文字暗示, 实际确认仍由
      [`RollbackConfirmBox`] 二次拦截
    - 仅有 "关闭" 按钮 (无 yes/no 决策语义), 用户点动作卡时直接 emit 信号并关闭
    """

    redetect_clicked = Signal()
    force_update_napcat_clicked = Signal()
    force_reinstall_linuxqq_clicked = Signal()
    rollback_clicked = Signal()

    def __init__(self, profile: ServerProfile, parent: QWidget) -> None:
        super().__init__(parent=parent)
        self._profile = profile
        # W10b-Maintenance: 按 backend_flavor 切换文案.
        # - NC: "NapCat 版本" / "强制更新 NapCat" / "清空远端 NapCat"
        # - SL: "SnowLuma 版本" / "重新部署 SnowLuma.Framework" / "清空远端 SnowLuma"
        # 两者 LinuxQQ 重装 / 回滚 中的文案不变 (与 LinuxQQ 路径同名).
        is_sl = profile.backend_flavor == BackendFlavor.SNOWLUMA
        self._is_sl = is_sl

        # ---------------- 标题 ----------------
        self.title_label = TitleLabel(self.tr(f"维护服务器: {profile.name}"), self)

        # ---------------- 信息区 (Form) ----------------
        info_section_label = StrongBodyLabel(self.tr("当前状态"), self)

        info_form = QFormLayout()
        info_form.setContentsMargins(0, 0, 0, 0)
        info_form.setHorizontalSpacing(16)
        info_form.setVerticalSpacing(6)

        state_text = _DEPLOYMENT_STATE_TEXT.get(profile.deployment_state, "未知")
        info_form.addRow(BodyLabel(self.tr("部署状态:"), self), BodyLabel(state_text, self))
        if is_sl:
            # SL: 主要版本字段是 SnowLuma.Framework, NapCat 不装
            info_form.addRow(
                BodyLabel(self.tr("SnowLuma 版本:"), self),
                BodyLabel(
                    profile.snowluma_framework_version or self.tr("未探测"),
                    self,
                ),
            )
        else:
            info_form.addRow(
                BodyLabel(self.tr("NapCat 版本:"), self),
                BodyLabel(profile.napcat_version or self.tr("未探测"), self),
            )
        info_form.addRow(
            BodyLabel(self.tr("LinuxQQ 版本:"), self),
            BodyLabel(profile.qq_version or self.tr("未探测"), self),
        )

        info_form_holder = QWidget(self)
        info_form_holder.setLayout(info_form)

        # ---------------- 动作区 ----------------
        action_section_label = StrongBodyLabel(self.tr("可用操作"), self)

        self.redetect_card = PushSettingCard(
            self.tr("刷新"),
            FI.SEARCH,
            self.tr("刷新版本信息"),
            (
                self.tr("重新探测远端的 SnowLuma.Framework / LinuxQQ 版本号, 不会重跑安装脚本")
                if is_sl
                else self.tr("重新探测远端的 NapCat / LinuxQQ 版本号, 不会重跑安装脚本")
            ),
            self,
        )
        if is_sl:
            self.force_update_card = PushSettingCard(
                self.tr("执行"),
                FI.UPDATE,
                self.tr("重新部署 SnowLuma.Framework"),
                self.tr(
                    "重新上传 Desktop 内置的 SnowLuma.Framework lite tarball 并重跑"
                    " install_snowluma 脚本 (同时重传 launcher 脚本, 修复本地脚本迭代后"
                    "远端没同步的问题)"
                ),
                self,
            )
        else:
            self.force_update_card = PushSettingCard(
                self.tr("执行"),
                FI.UPDATE,
                self.tr("强制更新 NapCat"),
                self.tr("重新拉取最新版本并重跑 install_napcat 脚本"),
                self,
            )
        self.force_reinstall_card = PushSettingCard(
            self.tr("执行"),
            FI.SYNC,
            self.tr("强制重装 LinuxQQ"),
            (
                self.tr("重新下载并安装 LinuxQQ; 比单独重新部署 Framework 影响更大")
                if is_sl
                else self.tr("重新下载并安装 LinuxQQ; 比单独更新 NapCat 影响更大")
            ),
            self,
        )
        self.rollback_card = PushSettingCard(
            self.tr("回滚"),
            FI.DELETE,
            self.tr("回滚部署 (危险)"),
            (
                self.tr("清空远端 SnowLuma 安装 (framework / launcher / runtime), 用于失败重置或彻底卸载")
                if is_sl
                else self.tr("清空远端 NapCat 安装, 用于失败重置或彻底卸载")
            ),
            self,
        )
        self.rollback_card.button.setObjectName("maintenanceRollbackBtn")

        # ---------------- 启用 / 禁用规则 ----------------
        # 维护类操作仅 DEPLOYED 可用; 回滚 DEPLOYED + FAILED 都可用
        is_deployed = profile.deployment_state is DeploymentState.DEPLOYED
        can_rollback = profile.deployment_state in (
            DeploymentState.DEPLOYED,
            DeploymentState.FAILED,
        )
        self.redetect_card.button.setEnabled(is_deployed)
        self.force_update_card.button.setEnabled(is_deployed)
        self.force_reinstall_card.button.setEnabled(is_deployed)
        self.rollback_card.button.setEnabled(can_rollback)

        # ---------------- 组装 ----------------
        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addSpacing(4)
        self.viewLayout.addWidget(info_section_label)
        self.viewLayout.addWidget(info_form_holder)
        self.viewLayout.addSpacing(8)
        self.viewLayout.addWidget(action_section_label)
        self.viewLayout.addWidget(self.redetect_card)
        self.viewLayout.addWidget(self.force_update_card)
        self.viewLayout.addWidget(self.force_reinstall_card)
        self.viewLayout.addWidget(self.rollback_card)

        self.widget.setMinimumSize(560, 460)

        # ---------------- 按钮: 仅 "关闭" ----------------
        self.yesButton.hide()
        self.cancelButton.setText(self.tr("关闭"))

        # ---------------- 信号绑定 ----------------
        # 卡片按钮点击 -> emit 信号 -> 关闭对话框 (业务层负责后续二次确认 / 启动 runner)
        self.redetect_card.button.clicked.connect(self._handle_redetect)
        self.force_update_card.button.clicked.connect(self._handle_force_update)
        self.force_reinstall_card.button.clicked.connect(self._handle_force_reinstall)
        self.rollback_card.button.clicked.connect(self._handle_rollback)

    # ---------- 内部 ----------
    def _handle_redetect(self) -> None:
        self.redetect_clicked.emit()
        self.accept()

    def _handle_force_update(self) -> None:
        self.force_update_napcat_clicked.emit()
        self.accept()

    def _handle_force_reinstall(self) -> None:
        self.force_reinstall_linuxqq_clicked.emit()
        self.accept()

    def _handle_rollback(self) -> None:
        self.rollback_clicked.emit()
        self.accept()


class RollbackConfirmBox(MessageBoxBase):
    """远端部署回滚的破坏性确认对话框.

    UX 要点:
    - 标题红色高亮 + 警告说明, 让用户明确这是不可逆操作
    - 默认勾选 "同时清理 LinuxQQ", 与 [`ServerManager.rollback_server`](src/core/remote/server_manager.py)
      ``include_qq=True`` 默认行为一致
    - 默认按钮不是"确认", 用户需要主动点击; 同时把"取消"作为视觉默认
    """

    def __init__(self, server_name: str, parent: QWidget, *, is_snowluma: bool = False) -> None:
        super().__init__(parent=parent)
        self._is_sl = is_snowluma

        self.title_label = TitleLabel(self.tr("确认回滚远端部署"), self)
        self.title_label.setStyleSheet("color: #d83b01; font-weight: 700;")

        # W10b-Maintenance: 按 flavor 切换文案
        if is_snowluma:
            content_text = (
                f"即将清空服务器 “{server_name}” 上的 SnowLuma 安装。\n\n"
                f"此操作不可逆, 远端工作区下的 SnowLuma.Framework / launcher / runtime "
                f"将被删除, 已运行的 SnowLuma Bot 也会被强制停止。\n"
                f"用于“重置环境后重新部署”或“清理失败的部署残留”."
            )
            hint_text = (
                "勾选后会一并删除 LinuxQQ + 便携式 node 缓存; 不勾选则仅清理"
                " SnowLuma.Framework, 保留 LinuxQQ 以便后续仅重新部署 Framework."
            )
        else:
            content_text = (
                f"即将清空服务器 “{server_name}” 上的 NapCat 安装。\n\n"
                f"此操作不可逆, 远端工作区下的 NapCat 将被删除, 已运行的 Bot 也会被强制停止。\n"
                f"用于“重置环境后重新部署”或“清理失败的部署残留”."
            )
            hint_text = (
                "勾选后会一并删除 LinuxQQ; 不勾选则仅清理 NapCat, 保留 LinuxQQ 以便后续仅重装 NapCat."
            )

        self.content_label = BodyLabel(self.tr(content_text), self)
        self.content_label.setWordWrap(True)

        self.include_qq_checkbox = CheckBox(self.tr("同时清理 LinuxQQ 安装与下载缓存"), self)
        self.include_qq_checkbox.setChecked(True)

        self.hint_label = CaptionLabel(self.tr(hint_text), self)
        self.hint_label.setStyleSheet("color: #8a8a8a;")
        self.hint_label.setWordWrap(True)

        self.widget.setMinimumSize(480, 280)
        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addWidget(self.content_label)
        self.viewLayout.addWidget(self.include_qq_checkbox)
        self.viewLayout.addWidget(self.hint_label)

        # 按钮文案: "确认回滚" / "取消"
        self.yesButton.setText(self.tr("确认回滚"))
        self.cancelButton.setText(self.tr("取消"))
        # 取消作为视觉默认, 防止"回车"误触发
        self.cancelButton.setDefault(True)
        self.yesButton.setDefault(False)

    def get_include_qq(self) -> bool:
        """返回用户选择的 ``include_qq`` 值."""
        return self.include_qq_checkbox.isChecked()
