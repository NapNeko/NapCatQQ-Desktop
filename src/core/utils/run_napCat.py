# -*- coding: utf-8 -*-
"""
## 运行 NapCat 流程
"""
from PySide6.QtCore import QProcess

# 项目内模块导入
from src.core.config.config_model import Config
from src.core.utils.path_func import PathFunc


def create_napcat_process(config: Config) -> QProcess:
    """创建并配置 QProcess

    Args:
        config (Config): 配置对象

    Returns:
        QProcess: 配置好的 QProcess 对象
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
            str(PathFunc().get_qq_path() / "QQ.exe"),
            str(PathFunc().napcat_path / "NapCatWinBootHook.dll"),
            config.bot.qq_id,
        ]
    )

    return process
