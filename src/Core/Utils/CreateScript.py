# -*- coding: utf-8 -*-
import json
import textwrap
from typing import Tuple
from pathlib import Path

from creart import it
from PySide6.QtGui import QDesktopServices
from PySide6.QtCore import Qt, QUrl, QObject, QOperatingSystemVersion
from qfluentwidgets import InfoBar, FluentIcon, MessageBox, InfoBarPosition, TransparentPushButton

from src.Core.PathFunc import PathFunc
from src.Core.Config.ConfigModel import Config, ScriptType


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

    def _verifySystemSupports(self, scriptType: ScriptType) -> ScriptType | None:
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
            return None
        else:
            if systemType == QOperatingSystemVersion.OSType.Windows:
                # QOperatingSystemVersion 没有 Linux Type
                self._showErrorBar(
                    self.infoBarParent.tr("Unable to create scripts"),
                    self.infoBarParent.tr(".sh scripts are not supported by the current system"),
                )
                return None
            return scriptType

    def createPs1Script(self) -> None:
        """
        创建 ps1 脚本
        """
        if self.config is None or self.scriptType is None:
            # 如果为 None 则中断创建
            return
        start_script_path, bot_config_path, napcat_config_path = self._pathVerify(ScriptType.PS1.value)
        if start_script_path is None:
            # 如果为 None 则中断创建
            return

        # 脚本内容
        ps1_script = f"""
        # This is a generated PowerShell script
        {
        f'$env:FFMPEG_PATH="{self.config.advanced.ffmpegPath}"'
        if self.config.advanced.ffmpegPath else ''
        }
        $QQpath = "{Path(self.config.advanced.QQPath) / 'QQ.exe'}"
        $command = f"chcp 65001; &'$QQpath' --enable-logging -q {self.config.bot.QQID}"
        $env:ELECTRON_RUN_AS_NODE = 1
        Start-Process powershell -ArgumentList "-noexit", "-noprofile", "-command", $command
        """
        # 创建配置文件
        self._createConfig(bot_config_path, napcat_config_path)

        # 写入脚本
        self._createScript(start_script_path, ps1_script)

    def createBatScript(self) -> None:
        """
        创建 Bat 脚本
        """
        # 验证路径和配置文件
        if self.config is None or self.scriptType is None:
            # 如果为 None 则中断创建
            return
        start_script_path, bot_config_path, napcat_config_path = self._pathVerify(ScriptType.BAT.value)
        if start_script_path is None:
            # 如果为 None 则中断创建
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
        set QQID="{self.config.bot.QQID}"
        set ELECTRON_RUN_AS_NODE=1
        !QQpath! --enable-logging -q !QQID!
        """

        # 创建配置文件
        self._createConfig(bot_config_path, napcat_config_path)

        # 写入脚本
        self._createScript(start_script_path, bat_script)

    def createShScript(self) -> None:
        """
        创建 sh 脚本（暂未进行测试,当 windows 环境下功能完成开发后再进行测试）
        """
        # 验证路径和配置文件
        if self.config is None or self.scriptType is None:
            # 如果为 None 则中断创建
            return
        start_script_path, bot_config_path, napcat_config_path = self._pathVerify(ScriptType.SH.value)
        if start_script_path is None:
            # 如果为 None 则中断创建
            return

        # 脚本内容
        sh_script = f"""
        #!/bin/bash
        {
        f'set FFMPEG_PATH="{self.config.advanced.ffmpegPath}"'
        if self.config.advanced.ffmpegPath else ''
        }
        export ELECTRON_RUN_AS_NODE=1
        {self.config.advanced.QQPath} {it(PathFunc).getNapCatPath() / "napcat.mjs"} -q {self.config.bot.QQID}
        """

        # 创建配置文件
        self._createConfig(bot_config_path, napcat_config_path)

        # 写入脚本
        self._createScript(start_script_path, sh_script)

    def _pathVerify(self, scriptType: str) -> Tuple[Path | None, Path, Path]:
        """
        检查所需路径是否存在
        """
        # 指定脚本创建位置和 NapCatQQ 的配置文件路径
        path = Path(self.config.advanced.startScriptPath) / self.config.bot.QQID
        config_path = it(PathFunc).getNapCatPath() / "config"

        # 验证路径是否存在
        if not path.exists() or path.is_dir():
            path.mkdir(parents=True, exist_ok=True)
        if not config_path.exists() or path.is_dir():
            path.mkdir(parents=True, exist_ok=True)

        # 指定脚本文件路径, bot配置文件路径, napcat配置文件路径
        start_script_path = path / f"start.{scriptType}"
        bot_config_path = config_path / f"onebot11_{self.config.bot.QQID}.json"
        napcat_config_path = config_path / f"napcat_{self.config.bot.QQID}.json"

        # 如果脚本文件已经存在则提示用户是否需要覆盖
        if start_script_path.exists():
            if not self._showOverlayPrompts(start_script_path):
                # 如果不覆盖则返回 None 后续用于结束创建
                start_script_path = None

        return start_script_path, bot_config_path, napcat_config_path

    def _createScript(self, start_script_path: Path, script: str) -> None:
        """
        创建脚本
        """
        with open(str(start_script_path), "w", encoding="utf-8") as f:
            f.write(textwrap.dedent(script).strip())

        # 写入脚本成功后提示
        self._showSuccessBar(start_script_path)

    def _createConfig(self, bot_config_path: Path, napcat_config_path: Path) -> None:
        """
        创建 napcat 的配置文件
        """
        # 如果 bot 配置文件或者 napcat 配置文件已经存在
        # 询问用户是否需要覆盖, 不覆盖则直接返回
        if bot_config_path.exists():
            if self._showOverlayPrompts(bot_config_path) is None:
                return
        if napcat_config_path.exists():
            if self._showOverlayPrompts(napcat_config_path) is None:
                return

        bot_config = {
            "http": {
                "enable": self.config.connect.http.enable,
                "host": self.config.connect.http.host,
                "port": self.config.connect.http.port,
                "secret": self.config.connect.http.secret,
                "enableHeart": self.config.connect.http.enableHeart,
                "enablePost": self.config.connect.http.enablePost,
                "postUrls": [str(url) for url in self.config.connect.http.postUrls],
            },
            "ws": {
                "enable": self.config.connect.ws.enable,
                "host": self.config.connect.ws.host,
                "port": self.config.connect.ws.port,
            },
            "reverseWs": {
                "enable": self.config.connect.reverseWs.enable,
                "urls": [str(url) for url in self.config.connect.reverseWs.urls],
            },
            "GroupLocalTime": {
                "Record": self.config.advanced.GroupLocalTime.Record,
                "RecordList": self.config.advanced.GroupLocalTime.RecordList,
            },
            "debug": self.config.advanced.debug,
            "heartInterval": self.config.bot.heartInterval,
            "messagePostFormat": self.config.bot.messagePostFormat,
            "enableLocalFile2Url": self.config.advanced.localFile2url,
            "musicSignUrl": self.config.bot.musicSignUrl,
            "reportSelfMessage": self.config.bot.reportSelfMsg,
            "token": self.config.bot.accessToken,
        }

        napcat_config = {
            "fileLog": self.config.advanced.fileLog,
            "consoleLog": self.config.advanced.consoleLog,
            "fileLogLevel": self.config.advanced.fileLogLevel,
            "consoleLogLevel": self.config.advanced.consoleLogLevel,
        }

        # 写入配置文件
        with open(str(bot_config_path), "w", encoding="utf-8") as f:
            json.dump(bot_config, f, indent=4)
        with open(str(napcat_config_path), "w", encoding="utf-8") as f:
            json.dump(napcat_config, f, indent=4)

    def _showOverlayPrompts(self, path: str | Path) -> int:
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
