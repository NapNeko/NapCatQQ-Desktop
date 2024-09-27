# -*- coding: utf-8 -*-
"""
## 操作 bot 配置文件的操作流程(主要是包含一些工具函数)
"""
# 标准库导入
import json
from json import JSONDecodeError
from typing import List

# 第三方库导入
from creart import it
from loguru import logger

# 项目内模块导入
from src.Core.Utils.PathFunc import PathFunc
from src.Core.Config.ConfigModel import Config, NapCatConfig, OneBotConfig


def read_config() -> List[Config]:
    """
    ## 读取 NCD 保存的机器人配置文件

    ## 返回
        - List[Config] 一个列表, 成员为 Config
    """
    try:
        with open(str(it(PathFunc).bot_config_path), "r", encoding="utf-8") as file:
            return [Config(**config) for config in json.load(file)]
    except Exception as error:
        logger.error(f"读取配置文件时引发错误: {error}")
        write_config([])  # 覆盖原有配置文件
        return []


def write_config(configs: List[Config]) -> None:
    """
    ## 写入 NCD 机器人配置文件
    """
    with open(str(it(PathFunc).bot_config_path), "w", encoding="utf-8") as file:
        json.dump([json.loads(config.json()) for config in configs], file, indent=4, ensure_ascii=False)


def check_duplicate_bot(config: Config) -> bool:
    """
    ## 检查是否已存在相同的机器人配置

    ## 参数
         - config 传入的机器人配置

    ## 返回
         - bool 类型
    """
    # 类型注解
    configs: List[Config]

    # 遍历配置文件列表进行判断, 如果QQID相同则代表存在相同配置
    for bot_config in read_config():
        if config.bot.QQID == bot_config.bot.QQID:
            return True
    return False


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
        configs: List[Config] = read_config()

        # 检查配置是否存在, 如果存在则更新, 不存在则追加到列表中
        for index, cfg in enumerate(configs):
            if cfg.bot.QQID == config.bot.QQID:
                configs[index] = config
                break
        else:
            configs.append(config)

        # 写入配置文件
        write_config(configs)

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
            json.dump(json.loads(onebot_config.json()), onebot_file, indent=4, ensure_ascii=False)
        with open(str(napcat_config_path), "w", encoding="utf-8") as napcat_file:
            json.dump(json.loads(napcat_config.json()), napcat_file, indent=4, ensure_ascii=False)

        return True

    except (FileNotFoundError, PermissionError, JSONDecodeError, KeyError, TypeError, Exception) as error:
        logger.error(f"在写入配置文件时引发 {type(error).__name__}: {error}")
        return False


def delete_config(config: Config) -> bool:
    """
    ## 删除配置文件

    ## 参数
         - config 传入的机器人配置

    ## 返回
         - bool 类型
    """
    try:
        configs: List[Config] = read_config()
        configs.remove(config)
        write_config(configs)
        return True
    except Exception as error:
        logger.error(f"在写入配置文件时引发 {type(error).__name__}: {error}")
        return False
