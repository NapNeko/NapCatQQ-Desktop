# -*- coding: utf-8 -*-

# 标准库导入
import smtplib
from dataclasses import dataclass
from datetime import datetime
from email.mime.multipart import MIMEMultipart
from email.mime.text import MIMEText
from enum import Enum
from string import Template

from PySide6.QtCore import QFile, QObject, QRunnable, Signal

# 项目内模块导入
from src.core.config import cfg
from src.core.config.config_model import Config
from src.core.qt.file import QFluentFile
from src.core.logging import LogSource, LogType, logger


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
    """存储发送邮件所需的数据

    Attributes
        email_content - 邮件内容
        msg_subject - 邮件主题
        msg_from - 发件人
        msg_to - 收件人
        sender_email - 发件邮箱
        reciver_email - 收件邮箱
        token - 发件邮箱授权码
        smtp_server - SMTP服务器地址
        smtp_port - SMTP服务器端口
        encryption - 加密类型
    """

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

    def __init__(self, email_content: str, msg_subject: str) -> None:
        """初始化 EmailData, 动态从配置中获取值

        Args:
            email_content (str): 邮件内容
            msg_subject (str): 邮件主题
        """
        self.email_content = email_content
        self.msg_subject = msg_subject

        # 动态从配置中获取值
        self.sender_email = cfg.get(cfg.email_sender)
        self.reciver_email = cfg.get(cfg.email_receiver)
        self.token = cfg.get(cfg.email_token)
        self.smtp_server = cfg.get(cfg.email_stmp_server)
        self.smtp_port = cfg.get(cfg.email_stmp_port)
        self.encryption = EncryptionType.get_enum_by_value(cfg.get(cfg.email_encryption))

        self.msg_from = f"NapCatQQ-Desktop Bot {self.sender_email}"
        self.msg_to = self.reciver_email


class Email(QObject, QRunnable):
    """发送邮件类

    继承 QObject 和 QRunnable 用于多线程发送邮件
    相对于 QThread 更加轻量级, 适合短时间任务
    """

    error_signal = Signal(str)
    success_signal = Signal(str)

    def __init__(self, data: EmailData) -> None:
        """初始化 Email 任务

        Args:
            data (EmailData): 发送邮件所需的数据
        """
        QObject.__init__(self)
        QRunnable.__init__(self)
        self.data = data

        # 自动销毁
        self.setAutoDelete(True)

    def run(self) -> None:
        """发送邮件

        构建邮件内容并发送, 支持 SSL/TLS/无加密 三种方式
        发送成功后发出 success_signal 信号, 发送失败发出 error_signal 信号
        该方法将在独立线程中运行, 不会阻塞主线程
        """
        logger.info(
            (
                "开始发送邮件: "
                f"server={self.data.smtp_server}:{self.data.smtp_port}, "
                f"receiver_configured={bool(self.data.reciver_email)}, encryption={self.data.encryption.name}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )
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
            logger.info(
                f"邮件发送成功: receiver_configured={bool(self.data.reciver_email)}, subject={self.data.msg_subject}",
                LogType.NETWORK,
                LogSource.CORE,
            )

        except smtplib.SMTPResponseException as e:
            self.error_signal.emit(f"创建邮件时出现错误, 请检查是否发件成功: {e.smtp_code} {e.smtp_error}")
            logger.exception(
                (
                    "邮件发送失败(SMTPResponseException): "
                    f"server={self.data.smtp_server}:{self.data.smtp_port}, "
                    f"receiver_configured={bool(self.data.reciver_email)}"
                ),
                e,
                LogType.NETWORK,
                LogSource.CORE,
            )

        except Exception as e:
            self.error_signal.emit(f"发送邮件时发生错误: {str(e)}")
            logger.exception(
                (
                    "邮件发送失败: "
                    f"server={self.data.smtp_server}:{self.data.smtp_port}, "
                    f"receiver_configured={bool(self.data.reciver_email)}"
                ),
                e,
                LogType.NETWORK,
                LogSource.CORE,
            )


def create_test_email_task() -> Email:
    """构建测试邮件任务, 由调用方决定如何处理信号和启动任务"""
    email_content = ""

    with QFluentFile(":template/template/email/test_email.html", QFile.OpenModeFlag.ReadOnly) as file:
        email_content = Template(bytes(file.readAll().data()).decode("utf-8")).safe_substitute(
            {"disconnect_time": datetime.now().strftime("%Y/%m/%d %H:%M:%S")}
        )

    if not email_content:
        raise ValueError("邮件内容不能为空, 无法创建测试邮件任务")

    return Email(
        EmailData(
            email_content=email_content,
            msg_subject="测试 NapCatQQ-Desktop 发件功能",
        )
    )


def create_offline_email_task(config: Config) -> Email:
    """构建离线通知邮件任务, 由调用方决定如何处理信号和启动任务"""
    email_content = ""

    with QFluentFile(":template/template/email/bot_offline_notice.html", QFile.OpenModeFlag.ReadOnly) as file:
        email_content = Template(bytes(file.readAll().data()).decode("utf-8")).safe_substitute(
            {
                "bot_name": f"{config.bot.name} ({config.bot.QQID})",
                "disconnect_time": datetime.now().strftime("%Y/%m/%d %H:%M:%S"),
            }
        )

    if not email_content:
        raise ValueError("邮件内容不能为空, 无法创建离线通知邮件任务")

    return Email(
        EmailData(
            email_content=email_content,
            msg_subject="NapCatQQ-Desktop 机器人离线通知",
        )
    )
