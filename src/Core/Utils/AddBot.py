# -*- coding: utf-8 -*-
"""
## 添加 Bot 到配置文件的操作流程
"""
import json
from json import JSONDecodeError
from typing import List

from creart import it
from loguru import logger

from src.Core.Utils.PathFunc import PathFunc
from src.Core.Config.ConfigModel import Config, NapCatConfig, OneBotConfig


def check_duplicate_bot(config: Config) -> bool:
    """
    ## 检查是否已存在相同的机器人配置

    ## 参数
         - config 传入的机器人配置

    ## 返回
         - bool 类型
    """
    with open(str(it(PathFunc).bot_config_path), "r", encoding="utf-8") as file:
        # 从配置文件读取配置, 解析成 Config
        bot_configs: List[Config] = [Config(**cfg) for cfg in json.load(file)]

    for bot_config in bot_configs:
        # 遍历配置文件列表进行判断, 如果QQID相同则代表存在相同配置
        if config.bot.QQID == bot_config.bot.QQID:
            return True


def update_config(config: Config) -> bool:
    """
    ## 更新配置到配置文件

    ## 参数
         - config : 传入的机器人配置

    ## 参数
         - bool : 通过返回 bool 判断是否更新成功

    """
    try:
        # 更新 NCD 中的配置文件
        with open(str(it(PathFunc).bot_config_path), "r", encoding="utf-8") as file:
            # 从配置文件读取配置, 解析成 Config
            bot_configs: List[Config] = [Config(**cfg) for cfg in json.load(file)]

        # 追加到列表中
        bot_configs.append(config)

        with open(str(it(PathFunc).bot_config_path), "w", encoding="utf-8") as file:
            # 写入配置文件
            json.dump([config.dict() for config in bot_configs], file, indent=4, ensure_ascii=False)

        # 定义配置内容
        onebot_config = OneBotConfig(
            **{
                "http": config.connect.http,
                "ws": config.connect.ws,
                "reverseWs": config.connect.reverseWs,
                "debug": config.advanced.debug,
                "heartInterval": config.bot.heartInterval,
                "messagePostFormat": config.bot.messagePostFormat,
                "enableLocalFile2Url": config.advanced.enableLocalFile2Url,
                "musicSignUrl": config.bot.musicSignUrl,
                "reportSelfMessage": config.bot.reportSelfMessage,
                "token": config.bot.token,
            }
        )
        napcat_config = NapCatConfig(
            **{
                "fileLog": config.advanced.fileLog,
                "consoleLog": config.advanced.consoleLog,
                "fileLogLevel": config.advanced.fileLogLevel,
                "consoleLogLevel": config.advanced.consoleLogLevel,
            }
        )

        # 更新 NC 中配置文件
        onebot_config_path = it(PathFunc).getNapCatPath() / "config" / f"onebot11_{config.bot.QQID}.json"
        napcat_config_path = it(PathFunc).getNapCatPath() / "config" / f"napcat_{config.bot.QQID}.json"
        with open(str(onebot_config_path), "w", encoding="utf-8") as onebot_file:
            json.dump(onebot_config.dict(), onebot_file, indent=4, ensure_ascii=False)
        with open(str(napcat_config_path), "w", encoding="utf-8") as napcat_file:
            json.dump(napcat_config.dict(), napcat_file, indent=4, ensure_ascii=False)

        return True

    except (FileNotFoundError, PermissionError, JSONDecodeError, KeyError, TypeError, Exception) as error:
        logger.error(f"在写入配置文件时引发 {type(error).__name__}: {error}")
        return False
