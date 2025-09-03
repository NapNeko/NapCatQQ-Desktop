# -*- coding: utf-8 -*-

# 标准库导入
import smtplib
from enum import Enum
from string import Template
from datetime import datetime
from dataclasses import dataclass
from email.mime.text import MIMEText
from email.mime.multipart import MIMEMultipart

# 第三方库导入
from qfluentwidgets.common.file import QFluentFile
from PySide6.QtCore import QFile, Signal, QObject, QRunnable, QThreadPool

# 项目内模块导入
from src.core.config import cfg
from src.ui.components.info_bar import error_bar, success_bar
from src.core.config.config_model import Config


class EncryptionType(Enum):
    """加密类型"""

    SSL = "SSL"
    TLS = "TLS"
    NONE = "无加密"

    @classmethod
    def get_values(cls):
        """获取所有枚举值的列表"""
        return [member.value for member in cls]

    @classmethod
    def get_names(cls):
        """获取所有枚举名称的列表"""
        return [member.name for member in cls]

    @classmethod
    def get_enum_by_value(cls, value: str):
        """通过值获取枚举"""
        for member in cls:
            if member.value == value:
                return member
        raise ValueError(f"No enum found for value: {value}")


@dataclass
class EmailData:
    email_content: str
    msg_subject: str
    msg_from: str
    msg_to: str
    sender_email: str
    reciver_email: str
    token: str
    smtp_server: str
    smtp_port: int
    encryption: EncryptionType

    def __init__(self, email_content: str, msg_subject: str):
        """
        初始化 EmailData, 动态从配置中获取值

        ## 参数
            email_content - 邮件内容
            msg_subject - 邮件主题
        """
        self.email_content = email_content
        self.msg_subject = msg_subject

        # 动态从配置中获取值
        self.sender_email = cfg.get(cfg.emailSender)
        self.reciver_email = cfg.get(cfg.emailReceiver)
        self.token = cfg.get(cfg.emailToken)
        self.smtp_server = cfg.get(cfg.emailStmpServer)
        self.smtp_port = cfg.get(cfg.emailStmpPort)
        self.encryption = EncryptionType.get_enum_by_value(cfg.get(cfg.emailEncryption))

        self.msg_from = f"NapCatQQ-Desktop Bot {self.sender_email}"
        self.msg_to = self.reciver_email


class Email(QObject, QRunnable):

    error_signal = Signal(str)
    success_signal = Signal(str)

    def __init__(self, data: EmailData) -> None:
        """
        # 参数
            data - EmailData 实例 各类所需要的数据
        """
        QObject.__init__(self)
        QRunnable.__init__(self)
        self.data = data

        # 自动销毁
        self.setAutoDelete(True)

    def run(self) -> None:
        """
        ## 发送邮件
        """
        msg = MIMEMultipart("related")
        msg["From"] = self.data.msg_from
        msg["To"] = self.data.msg_to
        msg["Subject"] = self.data.msg_subject

        # 创建HTML部分
        msg_html = MIMEText(self.data.email_content, "html", "utf-8")
        msg.attach(msg_html)

        try:
            if self.data.encryption == EncryptionType.SSL:
                with smtplib.SMTP_SSL(self.data.smtp_server, self.data.smtp_port) as server:
                    server.login(self.data.sender_email, self.data.token)
                    server.sendmail(self.data.sender_email, self.data.reciver_email, msg.as_string())

            elif self.data.encryption == EncryptionType.TLS:
                with smtplib.SMTP(self.data.smtp_server, self.data.smtp_port) as server:
                    server.starttls()
                    server.login(self.data.sender_email, self.data.token)
                    server.sendmail(self.data.sender_email, self.data.reciver_email, msg.as_string())

            else:
                with smtplib.SMTP(self.data.smtp_server, self.data.smtp_port) as server:
                    server.login(self.data.sender_email, self.data.token)
                    server.sendmail(self.data.sender_email, self.data.reciver_email, msg.as_string())
            self.success_signal.emit("邮件发送成功")

        except smtplib.SMTPResponseException as e:
            self.error_signal.emit(f"创建邮件时出现错误, 请检查是否发件成功: {e.smtp_code} {e.smtp_error}")

        except Exception as e:
            self.error_signal.emit(f"发送邮件时发生错误: {str(e)}")


def test_email():
    """测试邮件功能是否正常"""

    with QFluentFile(":template/template/email/test_email.html", QFile.OpenModeFlag.ReadOnly) as file:

        email_content = Template(file.readAll().data().decode("utf-8")).safe_substitute(
            {"disconnect_time": datetime.now().strftime("%Y/%m/%d %H:%M:%S")}
        )

    email = Email(
        EmailData(
            email_content=email_content,
            msg_subject="测试 NapCatQQ-Desktop 发件功能",
        )
    )

    email.error_signal.connect(lambda msg: error_bar(msg))
    email.success_signal.connect(lambda msg: success_bar(msg))

    QThreadPool.globalInstance().start(email)


def offline_email(config: Config):
    """离线通知"""

    with QFluentFile(":template/template/email/bot_offline_notice.html", QFile.OpenModeFlag.ReadOnly) as file:

        email_content = Template(file.readAll().data().decode("utf-8")).safe_substitute(
            {
                "bot_name": f"{config.bot.name} ({config.bot.QQID})",
                "disconnect_time": datetime.now().strftime("%Y/%m/%d %H:%M:%S"),
            }
        )

    email = Email(
        EmailData(
            email_content=email_content,
            msg_subject="NapCatQQ-Desktop 机器人离线通知",
        )
    )

    email.error_signal.connect(lambda msg: error_bar(msg))
    email.success_signal.connect(lambda msg: success_bar(msg))

    QThreadPool.globalInstance().start(email)
