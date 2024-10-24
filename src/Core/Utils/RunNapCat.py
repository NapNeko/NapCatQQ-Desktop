# -*- coding: utf-8 -*-
"""
## 运行 NapCat 流程
"""
# 第三方库导入
from creart import it
from PySide6.QtCore import QUrl, QProcess

# 项目内模块导入
from src.Core.Utils.PathFunc import PathFunc
from src.Core.NetworkFunc.Urls import Urls
from src.Core.Config.ConfigModel import Config


def create_napcat_process(config: Config) -> QProcess:
    """
    ## 创建并配置 QProcess

    ## 参数
        - config 机器人配置

    ## 返回
        - QProcess 程序实例
    """

    # 配置环境变量
    env = QProcess.systemEnvironment()
    env.append(f"NAPCAT_PATCH_PACKAGE={it(PathFunc).getNapCatPath() / 'qqnt.json'}")
    env.append(f"NAPCAT_LOAD_PATH={it(PathFunc).getNapCatPath() / 'loadNapCat.js'}")
    env.append(f"NAPCAT_INJECT_PATH={it(PathFunc).getNapCatPath() / 'NapCatWinBootHook.dll'}")
    env.append(f"NAPCAT_LAUNCHER_PATH={it(PathFunc).getNapCatPath() / 'NapCatWinBootMain.exe'}")
    env.append(f"NAPCAT_MAIN_PATH={it(PathFunc).getNapCatPath() / 'napcat.mjs'}")

    # 创建 QProcess 并配置
    process = QProcess()
    process.setEnvironment(env)
    process.setProgram(str(it(PathFunc).getNapCatPath() / "NapCatWinBootMain.exe"))
    process.setArguments(
        [
            str(it(PathFunc).getQQPath() / "QQ.exe"),
            str(it(PathFunc).getNapCatPath() / "NapCatWinBootHook.dll"),
            config.bot.QQID,
        ]
    )

    return process


def create_dlc_process(config: Config) -> QProcess:
    """
    ## 创建并配置 QProcess

    ## 参数
        - config 机器人配置

    ## 返回
        - QProcess 程序实例
    """
    # 获取参数
    host, port = config.advanced.packetServer.split(":")

    # 创建 QProcess 并配置
    process = QProcess()
    process.setProgram(str(it(PathFunc).dlc_path / Urls.NAPCATQQ_DLC_DOWNLOAD.value.fileName()))
    process.setArguments(["-ip", host, "-port", str(port)])

    return process
