# -*- coding: utf-8 -*-
# 标准库导入
import json
from typing import TYPE_CHECKING

# 第三方库导入
from qfluentwidgets import ExpandLayout
from qfluentwidgets import FluentIcon as FI
from qfluentwidgets import MessageBoxBase, PushButton, ScrollArea, SettingCardGroup, TitleLabel
from PySide6.QtCore import QObject, Qt, QThreadPool
from PySide6.QtWidgets import QGridLayout, QWidget

# 项目内模块导入
from src.core.config import cfg
from src.core.network.email import EncryptionType, create_test_email_task
from src.core.network.webhook import create_test_webhook_task
from src.core.utils.logger import LogSource, logger
from src.ui.common.card_surface import OpaqueOptionsSettingCard
from src.ui.components.info_bar import error_bar, success_bar, warning_bar
from src.ui.components.input_card.generic_card import (
    ComboBoxConfigCard,
    JsonTemplateEditConfigCard,
    LineEditConfigCard,
    ShowDialogCardBase,
    SwitchConfigCard,
    VersionInfoCard,
)

if TYPE_CHECKING:
    # 项目内模块导入
    pass


class General(ScrollArea):

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)
        # 创建控件
        self.view = QWidget()
        self.expand_layout = ExpandLayout(self.view)

        # 设置 ScrollArea 和控件
        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.view.setObjectName("SetupView")
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setObjectName("SetupGeneralWidget")

        # 调用方法
        self._create_config_cards()
        self._set_layout()

    def _create_config_cards(self) -> None:
        """创建配置项卡片"""
        # 创建组 - 行为
        self.action_group = SettingCardGroup(title=self.tr("行为"), parent=self.view)
        # 创建项
        self.close_button_card = OpaqueOptionsSettingCard(
            configItem=cfg.close_button_action,
            icon=FI.CLOSE,
            title=self.tr("关闭按钮"),
            content=self.tr("选择点击关闭按钮时的行为"),
            texts=[self.tr("关闭程序"), self.tr("最小化隐藏到托盘")],
            parent=self.action_group,
        )

        # 创建组 - 事件
        self.event_group = SettingCardGroup(title=self.tr("事件"), parent=self.view)
        # 创建项
        self.bot_offline_email_card = ShowDialogCardBase(
            dialog=BotOfflineEmailDialog,
            icon=FI.CHAT,
            title=self.tr("机器人离线通知[邮件]"),
            content=self.tr("设置机器人离线邮件通知"),
            parent=self.event_group,
        )
        self.bot_offline_webhook_card = ShowDialogCardBase(
            dialog=BotOfflineWebHookDialog,
            icon=FI.CHAT,
            title=self.tr("机器人离线通知[Webhook]"),
            content=self.tr("设置机器人离线Webhook通知"),
            parent=self.event_group,
        )

        # 创建组 - 版本
        self.version_group = SettingCardGroup(title=self.tr("版本"), parent=self.view)
        # 创建项
        self.ncd_version_card = VersionInfoCard(
            icon=FI.INFO,
            title=self.tr("NapCatQQ Desktop"),
            content=self.tr("当前 NapCatQQ Desktop 版本信息"),
            version=cfg.get(cfg.napcat_desktop_version),
            parent=self.version_group,
        )

    def _set_layout(self) -> None:
        """控件布局"""
        # 将卡片添加到组
        self.action_group.addSettingCard(self.close_button_card)
        self.event_group.addSettingCard(self.bot_offline_email_card)
        self.event_group.addSettingCard(self.bot_offline_webhook_card)
        self.version_group.addSettingCard(self.ncd_version_card)

        # 添加到布局
        self.expand_layout.addWidget(self.action_group)
        self.expand_layout.addWidget(self.event_group)
        self.expand_layout.addWidget(self.version_group)
        self.expand_layout.setContentsMargins(0, 0, 0, 0)
        self.view.setLayout(self.expand_layout)


class BotOfflineEmailDialog(MessageBoxBase):

    def __init__(self, parent: QObject) -> None:
        super().__init__(parent=parent)

        # 创建控件
        self.title_label = TitleLabel(self.tr("机器人离线通知[邮件]"), self)
        self.enable_card = SwitchConfigCard(FI.IOT, self.tr("启用邮箱通知"))
        self.receivers_card = LineEditConfigCard(FI.ROBOT, self.tr("收件人邮箱"), "Receivers@qq.com")
        self.sender_card = LineEditConfigCard(FI.ROBOT, self.tr("发件人邮箱"), "Sender@qq.com")
        self.token_card = LineEditConfigCard(FI.VPN, self.tr("发件人邮箱密钥"), "Token")
        self.stmp_server_card = LineEditConfigCard(FI.ALBUM, self.tr("SMTP服务器"), "smtp.qq.com")
        self.stmp_server_port_card = LineEditConfigCard(FI.ALBUM, self.tr("SMTP服务器端口"), "465")
        self.encryption_card = ComboBoxConfigCard(FI.VPN, self.tr("加密方式"), EncryptionType.get_values())
        self.test_email_button = PushButton(self.tr("发送测试邮件"))

        # 填充配置
        self.fill_config()

        # 布局
        self.grid_layout = QGridLayout()
        self.grid_layout.addWidget(self.enable_card, 0, 0, 1, 4)
        self.grid_layout.addWidget(self.receivers_card, 1, 0, 1, 4)
        self.grid_layout.addWidget(self.sender_card, 2, 0, 1, 4)
        self.grid_layout.addWidget(self.token_card, 3, 0, 1, 4)
        self.grid_layout.addWidget(self.stmp_server_card, 4, 0, 1, 2)
        self.grid_layout.addWidget(self.stmp_server_port_card, 4, 2, 1, 2)
        self.grid_layout.addWidget(self.encryption_card, 5, 0, 1, 4)
        self.grid_layout.setContentsMargins(0, 0, 0, 0)
        self.grid_layout.setSpacing(8)
        self.buttonLayout.addWidget(self.test_email_button, 1)

        # 设置布局
        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addLayout(self.grid_layout)
        self.widget.setMinimumSize(500, 400)

        # 链接信号
        self.test_email_button.clicked.connect(self._send_test_email)

    def fill_config(self) -> None:
        """填充配置"""
        self.enable_card.fill_value(cfg.get(cfg.bot_offline_email_notice))
        self.receivers_card.fill_value(cfg.get(cfg.email_receiver))
        self.sender_card.fill_value(cfg.get(cfg.email_sender))
        self.token_card.fill_value(cfg.get(cfg.email_token))
        self.stmp_server_card.fill_value(cfg.get(cfg.email_stmp_server))
        self.stmp_server_port_card.fill_value(str(cfg.get(cfg.email_stmp_port)))
        self.encryption_card.fill_value(cfg.get(cfg.email_encryption))

    def _collect_config_values(self) -> list[tuple[object, object]]:
        """收集并预校验待保存的邮件配置。"""
        try:
            smtp_port = int(self.stmp_server_port_card.get_value())
        except ValueError as error:
            raise ValueError("SMTP服务器端口必须为整数") from error

        values = [
            (cfg.bot_offline_email_notice, self.enable_card.get_value()),
            (cfg.email_receiver, self.receivers_card.get_value()),
            (cfg.email_sender, self.sender_card.get_value()),
            (cfg.email_token, self.token_card.get_value()),
            (cfg.email_stmp_server, self.stmp_server_card.get_value()),
            (cfg.email_stmp_port, smtp_port),
            (cfg.email_encryption, self.encryption_card.get_value()),
        ]

        for item, value in values:
            if not item.validator.validate(value):
                raise ValueError(f"配置项 {item.key} 的值无效")

        return values

    def save_config(self) -> bool:
        """保存配置。"""
        try:
            values = self._collect_config_values()
            for item, value in values:
                cfg.set(item, value)
            logger.info(
                (
                    "邮件通知配置已保存: "
                    f"enabled={self.enable_card.get_value()}, "
                    f"receiver_configured={bool(self.receivers_card.get_value())}, "
                    f"sender_configured={bool(self.sender_card.get_value())}, "
                    f"server={self.stmp_server_card.get_value()}:{self.stmp_server_port_card.get_value()}"
                ),
                log_source=LogSource.UI,
            )
            success_bar(self.tr("配置已保存"))
            self.fill_config()
            return True
        except ValueError as exc:
            logger.warning(f"邮件通知配置保存失败: {exc}", log_source=LogSource.UI)
            warning_bar(self.tr("配置保存失败，请检查输入是否正确"))
            return False

    def accept(self) -> None:
        """接受按钮"""
        if not self.save_config():
            return
        super().accept()

    def _send_test_email(self) -> None:
        """保存配置后发送测试邮件"""
        if not self.save_config():
            return

        logger.info("准备发送测试邮件", log_source=LogSource.UI)
        task = create_test_email_task()
        task.success_signal.connect(lambda msg: success_bar(self.tr(msg)))
        task.error_signal.connect(lambda msg: error_bar(msg))
        QThreadPool.globalInstance().start(task)


class BotOfflineWebHookDialog(MessageBoxBase):

    def __init__(self, parent: QObject) -> None:
        super().__init__(parent=parent)

        # 创建控件
        self.title_label = TitleLabel(self.tr("机器人离线通知[Webhook]"), self)
        self.enable_card = SwitchConfigCard(FI.IOT, self.tr("启用Webhook通知"))
        self.webhook_url_card = LineEditConfigCard(FI.ROBOT, self.tr("Webhook地址"), "https://example.com/webhook")
        self.webhook_secret_card = LineEditConfigCard(FI.VPN, self.tr("Webhook 密钥"), "Secret")
        self.json_card = JsonTemplateEditConfigCard(FI.CODE, self.tr("Webhook JSON"))
        self.test_webhook_buttonn = PushButton(self.tr("发送测试请求"), self)

        # 布局
        self.grid_layout = QGridLayout()
        self.grid_layout.addWidget(self.enable_card, 0, 0, 1, 4)
        self.grid_layout.addWidget(self.webhook_url_card, 1, 0, 1, 4)
        self.grid_layout.addWidget(self.webhook_secret_card, 2, 0, 1, 4)
        self.grid_layout.addWidget(self.json_card, 3, 0, 1, 4)
        self.grid_layout.setContentsMargins(0, 0, 0, 0)
        self.grid_layout.setSpacing(8)

        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addLayout(self.grid_layout)

        self.buttonLayout.addWidget(self.test_webhook_buttonn, 1)

        # 设置
        self.widget.setMinimumSize(650, 400)

        # 填充配置
        self.fill_config()

        # 链接信号
        self.test_webhook_buttonn.clicked.connect(self._send_test_webhook)

    def fill_config(self) -> None:
        """填充配置"""
        self.enable_card.fill_value(cfg.get(cfg.bot_offline_web_hook_notice))
        self.webhook_url_card.fill_value(cfg.get(cfg.web_hook_url))
        self.webhook_secret_card.fill_value(cfg.get(cfg.web_hook_secret))
        self.json_card.fill_value(cfg.get(cfg.web_hook_json))

    def _collect_config_values(self) -> list[tuple[object, object]]:
        """收集并预校验待保存的 WebHook 配置。"""
        json_payload = self.json_card.get_value()
        json.loads(json_payload)

        values = [
            (cfg.bot_offline_web_hook_notice, self.enable_card.get_value()),
            (cfg.web_hook_url, self.webhook_url_card.get_value()),
            (cfg.web_hook_secret, self.webhook_secret_card.get_value()),
            (cfg.web_hook_json, json_payload),
        ]

        for item, value in values:
            if not item.validator.validate(value):
                raise ValueError(f"配置项 {item.key} 的值无效")

        return values

    def save_config(self) -> bool:
        """保存配置。"""
        try:
            values = self._collect_config_values()
            for item, value in values:
                cfg.set(item, value)
            logger.info(
                (
                    "WebHook 通知配置已保存: "
                    f"enabled={self.enable_card.get_value()}, "
                    f"url_configured={bool(self.webhook_url_card.get_value())}, "
                    f"secret_configured={bool(self.webhook_secret_card.get_value())}, "
                    f"json_chars={len(self.json_card.get_value())}"
                ),
                log_source=LogSource.UI,
            )
            success_bar(self.tr("配置已保存"))
            self.fill_config()
            return True
        except (TypeError, ValueError, json.JSONDecodeError) as exc:
            logger.warning(f"WebHook 通知配置保存失败: {exc}", log_source=LogSource.UI)
            warning_bar(self.tr("配置保存失败，请检查输入是否正确"))
            return False

    def accept(self) -> None:
        """接受按钮"""
        if not self.save_config():
            return

        super().accept()

    def _send_test_webhook(self) -> None:
        """保存配置后发送测试请求"""
        if not self.save_config():
            return

        logger.info("准备发送测试 WebHook 请求", log_source=LogSource.UI)
        task = create_test_webhook_task()
        task.success_signal.connect(lambda msg: success_bar(self.tr(msg)))
        task.error_signal.connect(lambda msg: error_bar(msg))
        QThreadPool.globalInstance().start(task)
