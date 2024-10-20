# -*- coding: utf-8 -*-
"""
## WebUi 工具类
"""
# 标准库导入
import time

# 第三方库导入
import httpx
import qrcode
from PIL import Image, ImageOps
from loguru import logger
from qfluentwidgets import FluentThemeColor, FluentBackgroundTheme, isDarkTheme
from PySide6.QtCore import QUrl, Signal, QBuffer, QThread, QByteArray

# 项目内模块导入
from src.Core.Config import cfg
from src.Ui.common.info_bar import error_bar
from src.Core.Config.OperateConfig import read_webui_config


class WebUi(QThread):
    """
    ## WebUi 工具类
    """

    qrcode_bytes_single = Signal(QByteArray)
    login_state_single = Signal(bool)

    def __init__(self, url: QUrl) -> None:
        """
        ## 初始化
        """
        super().__init__()

        self.url = url
        self.webuiConfig = read_webui_config()
        self.token = self.webuiConfig.token
        self.loginState = False

        # 客户端链接
        self.client = httpx.Client(base_url=self.url.toString())

    def run(self) -> None:
        """
        ## 运行
        """
        time.sleep(3)
        self.getAuth()
        self.getQRCode()

        while not self.loginState:
            self.getLoginStatus()

    def getAuth(self) -> str:
        """
        ## 获取授权
        """
        try:
            if (response := self.client.post("/api/auth/login", json={"token": self.token})).status_code != 200:
                return
            self.auth = f"Bearer {response.json()['data']['Credential']}"  # 返回 Bearer Token
        except Exception:
            time.sleep(1)

    def getLoginStatus(self) -> None:
        """
        ## 获取登录状态
        """
        try:
            # 设置 headers，包括 Authorization
            headers = {
                "Authorization": self.auth,  # 返回的 Bearer Token 放在 headers 中
                "Accept": "*/*",
                "Content-Type": "application/json",
            }
            # 获取登录状态信息
            if (response := self.client.post("/api/QQLogin/CheckLoginStatus", headers=headers)).status_code != 200:
                return

            if response.json()["data"]["isLogin"]:
                self.loginState = True
                self.login_state_single.emit(True)
        except Exception:
            time.sleep(1)

    def getQRCode(self) -> None:
        """
        ## 获取二维码
        """
        try:
            # 设置 headers，包括 Authorization
            headers = {
                "Authorization": self.auth,  # 返回的 Bearer Token 放在 headers 中
                "Accept": "*/*",
                "Content-Type": "application/json",
            }
            # 获取登录链接
            if (response := self.client.post("/api/QQLogin/GetQQLoginQrcode", headers=headers)).status_code != 200:
                return

            # 生成二维码
            qr = qrcode.QRCode(version=1, error_correction=qrcode.constants.ERROR_CORRECT_Q, box_size=10, border=4)
            qr.add_data(response.json()["data"]["qrcode"])
            qr.make(fit=True)

            # 生成二维码图像（根据主题设置颜色）
            img = qr.make_image(
                fill_color="white" if isDarkTheme() else "black",  # 极夜主题为白色方块，明亮主题为黑色方块
                back_color="black" if isDarkTheme() else "white",  # 极夜主题为黑色背景，明亮主题为白色背景
            ).convert("RGBA")

            # 根据主题判断哪些像素需要转换为透明，并用列表推导式进行处理
            if isDarkTheme():
                # 极夜主题：将黑色背景 (0, 0, 0) 转为透明
                img.putdata([(r, g, b, 0) if (r, g, b) == (0, 0, 0) else (r, g, b, a) for r, g, b, a in img.getdata()])
            else:
                # 明亮主题：将白色背景 (255, 255, 255) 转为透明
                img.putdata(
                    [(r, g, b, 0) if (r, g, b) == (255, 255, 255) else (r, g, b, a) for r, g, b, a in img.getdata()]
                )

            # 创建字节流对象
            byte_array = QByteArray()
            buffer = QBuffer(byte_array)
            buffer.open(QBuffer.WriteOnly)

            # 将 Pillow Image 保存到 QBuffer 中 (PNG 格式)
            img.save(buffer, format="PNG")
            buffer.close()

            # 发送信号
            self.qrcode_bytes_single.emit(byte_array)
        except Exception as e:
            logger.error(f"获取二维码失败: {e}")
