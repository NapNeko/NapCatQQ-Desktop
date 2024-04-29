# -*- coding: utf-8 -*-
import json
import textwrap
from pathlib import Path

from PySide6.QtCore import QObject, QOperatingSystemVersion, Qt
from creart import it
from qfluentwidgets import InfoBar, InfoBarPosition

from src.Core.CreateScript.ConfigModel import Config, ScriptType
from src.Core.PathFunc import PathFunc


class CreateScript:
    """
    ## 创建启动脚本:
    传入配置文件和脚本类型生成脚本, 按需实例化
    """

    def __init__(self, config: dict, scriptType: ScriptType, infoBarParent: QObject) -> None:
        """
        ## 验证传入的参数:
        如果传入参数有问题,则会返回 false 并附带一个错误信息用于提示

        ### 参数:
            - config: 脚本所需的变量
            - scriptType: 脚本类型
        """
        self.infoBarParent = infoBarParent
        self.scriptType = self._verifySystemSupports(scriptType)
        self.config = self._verifyConfig(config)

    def _verifyConfig(self, config):
        try:
            return Config(**config)
        except ValueError as e:
            # 后续可能会细化 Error
            self._showErrorBar(self.infoBarParent.tr("Unable to create scripts"), str(e))

    def _verifySystemSupports(self, scriptType: ScriptType) -> ScriptType:
        """验证系统是否支持脚本

        Windows 平台不支持 sh脚本
        Linux 平台不支持 bat, ps1 脚本
        """
        systemType = QOperatingSystemVersion.currentType()

        if scriptType == ScriptType.BAT or scriptType == ScriptType.PS1:
            # 判断脚本类型
            if systemType == QOperatingSystemVersion.OSType.Windows:
                # 如果该类型脚本系统支持则验证成功,反之报错
                return scriptType
            self._showErrorBar(
                self.infoBarParent.tr("Unable to create scripts"),
                self.infoBarParent.tr("Currently, the system does not support the creation of bat, ps1 scripts"),
            )
        else:
            if systemType == QOperatingSystemVersion.OSType.Windows:
                # QOperatingSystemVersion 没有 Linux Type
                self._showErrorBar(
                    self.infoBarParent.tr("Unable to create scripts"),
                    self.infoBarParent.tr(".sh scripts are not supported by the current system"),
                )
            return scriptType

    def _pathVerify(self):
        """
        检查所需路径是否存在
        """
        path = it(PathFunc).start_script / self.config.bot.QQID
        config_path = it(PathFunc).getNapCatPath() / "config"
        if not path.exists() or path.is_dir():
            path.mkdir(parents=True, exist_ok=True)
        if not config_path.exists() or path.is_dir():
            path.mkdir(parents=True, exist_ok=True)

        config_path = config_path / f"onebot11_{self.config.bot.QQID}.json"

        return path, config_path

    def createPs1Script(self) -> None:
        """
        创建 ps1 脚本
        """
        if self.config is None:
            return
        path, config_path = self._pathVerify()

        # 脚本内容
        ps1_script = f"""
        # This is a generated PowerShell script
        {f'$env:FFMPEG_PATH="{self.config.advanced.ffmpegPath}"' if self.config.advanced.ffmpegPath else ''}
        $params = "-q {self.config.bot.QQID}"
        $QQpath = "{Path(self.config.advanced.QQPath) / 'QQ.exe'}"
        $Bootfile = "{it(PathFunc).getNapCatPath() / "napcat.cjs"}"
        $env:ELECTRON_RUN_AS_NODE = 1
        Start-Process powershell -ArgumentList "-noexit", "-noprofile", "-command &'$QQpath' $Bootfile $params"
        """
        # 写入脚本
        with open(str(path / "start.ps1"), "w", encoding="utf-8") as f:
            f.write(textwrap.dedent(ps1_script))

        # 创建配置文件
        self._createConfig(config_path)

    def _createConfig(self, config_path: Path) -> None:
        """
        创建 napcat 的配置文件
        """
        config = {
            "httpHost": self.config.bot.http.addresses,
            "enableHttp": self.config.bot.http.enable,
            "httpPort": self.config.bot.http.port,
            "enableHttpPost": self.config.bot.httpReport.enable,
            "enableHttpHeart": self.config.bot.httpReport.enableHeart,
            "httpSecret": self.config.bot.httpReport.token,
            "httpPostUrls": self.config.bot.httpReportUrls,
            "enableWs": self.config.bot.ws.enable,
            "wsHost": self.config.bot.ws.addresses,
            "wsPort": self.config.bot.ws.port,
            "enableWsReverse": self.config.bot.wsReverse,
            "wsReverseUrls": self.config.bot.wsReverseUrls,
            "messagePostFormat": self.config.bot.msgFormat,
            "reportSelfMessage": self.config.bot.reportSelfMsg,
            "debug": self.config.advanced.debug,
            "enableLocalFile2Url": self.config.advanced.localFile2url,
            "heartInterval": self.config.bot.heartInterval,
            "token": self.config.bot.accessToken,
        }

        # 写入配置文件
        with open(str(config_path), "w", encoding="utf-8") as f:
            json.dump(config, f, indent=4)

    def _showErrorBar(self, title: str, content: str) -> None:
        """显示错误消息条

        包装了 InfoBar.error, 使其不要重复传一些不必要的参数
        """
        InfoBar.error(
            title=title,
            content=content,
            orient=Qt.Orientation.Vertical,
            duration=50000,
            isClosable=True,
            position=InfoBarPosition.BOTTOM_RIGHT,
            parent=self.infoBarParent,
        )
