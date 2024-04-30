# -*- coding: utf-8 -*-
import json
import textwrap
from pathlib import Path

from PySide6.QtCore import QObject, QOperatingSystemVersion, Qt, QUrl
from PySide6.QtGui import QDesktopServices
from creart import it
from qfluentwidgets import InfoBar, InfoBarPosition, MessageBox, TransparentPushButton, FluentIcon

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

        Windows 平台不支持 sh 脚本
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

    def createPs1Script(self) -> None:
        """
        创建 ps1 脚本
        """
        if self.config is None:
            return
        start_script_path, config_path = self._pathVerify(ScriptType.PS1.value)
        if start_script_path is None:
            return

        # 脚本内容
        ps1_script = f"""
        # This is a generated PowerShell script
        {
        f'$env:FFMPEG_PATH="{self.config.advanced.ffmpegPath}"'
        if self.config.advanced.ffmpegPath else ''
        }
        $params = "-q {self.config.bot.QQID}"
        $QQpath = "{Path(self.config.advanced.QQPath) / 'QQ.exe'}"
        $Bootfile = "{it(PathFunc).getNapCatPath() / "napcat.cjs"}"
        $command = "chcp 65001; &'$QQpath' $Bootfile $params"
        $env:ELECTRON_RUN_AS_NODE = 1
        Start-Process powershell -ArgumentList "-noexit", "-noprofile", "-command", $command
        """
        # 创建配置文件
        self._createConfig(config_path)

        # 写入脚本
        self._createScript(start_script_path, ps1_script)

    def createBatScript(self) -> None:
        """
        创建 Bat 脚本
        """
        # 验证路径和配置文件
        if self.config is None:
            return
        start_script_path, config_path = self._pathVerify(ScriptType.BAT.value)
        if start_script_path is None:
            return

        # 脚本内容
        bat_script = f"""
        @echo off
        setlocal enabledelayedexpansion
        chcp 65001
        {
        f'set FFMPEG_PATH="{self.config.advanced.ffmpegPath}"'
        if self.config.advanced.ffmpegPath else ''
        }
        set QQPath="{Path(self.config.advanced.QQPath) / 'QQ.exe'}"
        set NapCatPath="{it(PathFunc).getNapCatPath() / "napcat.cjs"}"
        set QQID="{self.config.bot.QQID}"
        set ELECTRON_RUN_AS_NODE=1
        !QQpath! !NapCatPath! -q !QQID!
        """

        # 创建配置文件
        self._createConfig(config_path)

        # 写入脚本
        self._createScript(start_script_path, bat_script)

    def createShScript(self) -> None:
        """
        创建 sh 脚本（暂未进行测试,当 windows 环境下功能完成开发后再进行测试）
        """
        # 验证路径和配置文件
        if self.config is None:
            return
        start_script_path, config_path = self._pathVerify(ScriptType.SH.value)
        if start_script_path is None:
            return

        # 脚本内容
        sh_script = f"""
        #!/bin/bash
        {
        f'set FFMPEG_PATH="{self.config.advanced.ffmpegPath}"'
        if self.config.advanced.ffmpegPath else ''
        }
        export ELECTRON_RUN_AS_NODE=1
        {self.config.advanced.QQPath} {it(PathFunc).getNapCatPath() / "napcat.cjs"} -q {self.config.bot.QQID}
        """

        # 创建配置文件
        self._createConfig(config_path)

        # 写入脚本
        self._createScript(start_script_path, sh_script)

    def _pathVerify(self, scriptType: str) -> Path | None:
        """
        检查所需路径是否存在
        """
        path = it(PathFunc).start_script / self.config.bot.QQID
        config_path = it(PathFunc).getNapCatPath() / "config"
        if not path.exists() or path.is_dir():
            path.mkdir(parents=True, exist_ok=True)
        if not config_path.exists() or path.is_dir():
            path.mkdir(parents=True, exist_ok=True)

        start_script_path = path / f"start.{scriptType}"
        config_path = config_path / f"onebot11_{self.config.bot.QQID}.json"

        if start_script_path.exists():
            if not self._showOverlayPrompts(start_script_path):
                start_script_path = None

        return start_script_path, config_path

    def _createScript(self, start_script_path: Path, script: str) -> None:
        """
        创建脚本
        """
        with open(str(start_script_path), "w", encoding="utf-8") as f:
            f.write(textwrap.dedent(script).strip())

        # 写入脚本成功后提示
        self._showSuccessBar(start_script_path)

    def _createConfig(self, config_path: Path) -> None:
        """
        创建 napcat 的配置文件
        """
        if self._showOverlayPrompts(config_path) is None:
            return

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

    def _showOverlayPrompts(self, path: str) -> None:
        """
        提示用户已经存在文件,是否覆盖
        """
        box = MessageBox(
            title=self.infoBarParent.tr("Override actions"),
            content=self.infoBarParent.tr(
                "The following files already exist and will be overwritten "
                "if you need to continue creating\n\n"
                f"{path}"
            ),
            parent=self.infoBarParent,
        )
        box.yesButton.setText(self.infoBarParent.tr("Cover"))

        return box.exec()

    def _showSuccessBar(self, scriptPath: Path):
        """
        ## 显示创建成功消息条
        """
        info = InfoBar.success(
            title=self.infoBarParent.tr("Success"),
            content=self.infoBarParent.tr(f"Successfully created {scriptPath.name}"),
            orient=Qt.Orientation.Vertical,
            isClosable=True,
            duration=50000,
            parent=self.infoBarParent,
        )
        openFolderButton = TransparentPushButton(icon=FluentIcon.FOLDER, text=self.infoBarParent.tr("Open the folder"))
        openFolderButton.clicked.connect(lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(str(scriptPath.parent))))
        openFolderButton.clicked.connect(info.close)
        info.addWidget(openFolderButton)
        info.show()

    def _showErrorBar(self, title: str, content: str) -> None:
        """
        ## 显示错误消息条

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
