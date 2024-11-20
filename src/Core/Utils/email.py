# -*- coding: utf-8 -*-

# 标准库导入
import smtplib
from string import Template
from datetime import datetime
from email.mime.text import MIMEText
from email.mime.multipart import MIMEMultipart

from PySide6.QtCore import QFile, Signal, QThread

# 项目内模块导入
from src.Core.Config import cfg
from src.Core.Config.ConfigModel import Config


class Email(QThread):

    error_single = Signal(str)

    def __init__(self, config: Config) -> None:
        super().__init__()
        self.senderEmail = cfg.get(cfg.emailSender)
        self.receiversEmail = cfg.get(cfg.emailReceiver)
        self.token = cfg.get(cfg.emailToken)
        self.smtpServer = cfg.get(cfg.emailStmpServer)

        self.config = config

    def run(self) -> None:
        """
        ## 启动线程
        """
        # 解析 HTML 文件
        self.parsingHTML()
        # 发送邮件
        self.sendEmail()

    def parsingHTML(self) -> None:
        """
        ## 解析 HTML 内容
        """
        path = ":template/template/email/bot_offline_notice.html"
        if not (file := QFile(path)).open(QFile.OpenModeFlag.ReadOnly | QFile.OpenModeFlag.Text):
            self.error_single.emit(self.tr("创建邮件时引发错误: 模板文件缺失"))
            return

        self.template = Template(file.readAll().data().decode("utf-8")).safe_substitute(
            {
                "bot_name": f"{self.config.bot.name}({self.config.bot.QQID})",
                "disconnect_time": datetime.now().strftime("%Y/%m/%d %H:%M:%S"),
            }
        )
        file.close()

    def sendEmail(self) -> None:
        """
        ## 发件功能
        """
        # 创建邮件对象
        msg = MIMEMultipart("related")
        msg["From"] = f"NapCat Desktop <{self.senderEmail}>"
        msg["To"] = self.receiversEmail
        msg["Subject"] = "机器人掉线啦!"

        # 创建HTML部分
        msg_html = MIMEText(self.template, "html", "utf-8")
        msg.attach(msg_html)

        try:
            # 连接 SMTP 服务器并发送邮件
            with smtplib.SMTP_SSL(self.smtpServer, 465) as server:
                server.login(self.senderEmail, self.token)  # 登录SMTP服务器
                server.sendmail(self.senderEmail, self.receiversEmail, msg.as_string())  # 发送邮件
        except smtplib.SMTPResponseException as e:
            self.error_single.emit(self.tr(f"创建邮件时引发错误: {e.smtp_code} {e.smtp_error}"))
