# -*- coding: utf-8 -*-
"""
## 运行 NapCat 流程
"""
from PySide6.QtCore import QProcess

# 项目内模块导入
from src.Core.Utils.PathFunc import PathFunc
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
    env.append(f"NAPCAT_PATCH_PACKAGE={PathFunc().napcat_path / 'qqnt.json'}")
    env.append(f"NAPCAT_LOAD_PATH={PathFunc().napcat_path / 'loadNapCat.js'}")
    env.append(f"NAPCAT_INJECT_PATH={PathFunc().napcat_path / 'NapCatWinBootHook.dll'}")
    env.append(f"NAPCAT_LAUNCHER_PATH={PathFunc().napcat_path / 'NapCatWinBootMain.exe'}")
    env.append(f"NAPCAT_MAIN_PATH={PathFunc().napcat_path / 'napcat.mjs'}")

    # 创建 QProcess 并配置
    process = QProcess()
    process.setEnvironment(env)
    process.setProgram(str(PathFunc().napcat_path / "NapCatWinBootMain.exe"))
    process.setArguments(
        [
            str(PathFunc().getQQPath() / "QQ.exe"),
            str(PathFunc().napcat_path / "NapCatWinBootHook.dll"),
            config.bot.QQID,
        ]
    )

    return process
