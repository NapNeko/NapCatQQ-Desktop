# -*- coding: utf-8 -*-
"""
## 操作 bot 配置文件的操作流程(主要是包含一些工具函数)
"""
# 标准库导入
import json
import os
import uuid
from json import JSONDecodeError
from pathlib import Path
from typing import Any, List

# 第三方库导入
from creart import it

# 项目内模块导入
from src.core.config.config_model import Config, NapCatConfig, OneBotConfig
from src.core.utils.logger import logger
from src.core.utils.path_func import PathFunc


def _get_path_func() -> PathFunc:
    """获取路径处理实例。"""
    return it(PathFunc)


def _model_to_payload(model: Config | OneBotConfig | NapCatConfig) -> Any:
    """将 Pydantic 模型转换为可序列化 JSON 结构。"""
    return json.loads(model.model_dump_json())


def _next_transaction_path(path: Path, marker: str) -> Path:
    """生成同目录下的事务临时路径。"""
    return path.with_name(f"{path.name}.{marker}.{uuid.uuid4().hex}")


def _replace_path(src: Path, dst: Path) -> None:
    """替换目标路径，单独抽出便于测试故障注入。"""
    os.replace(src, dst)


def _cleanup_path(path: Path) -> None:
    """清理事务临时文件。"""
    try:
        if path.exists():
            path.unlink()
    except FileNotFoundError:
        pass


def _build_onebot_config(config: Config) -> OneBotConfig:
    """构造 NapCat OneBot 配置。"""
    return OneBotConfig(
        **{
            "network": config.connect,
            "musicSignUrl": config.bot.musicSignUrl,
            "enableLocalFile2Url": config.advanced.enableLocalFile2Url,
            "parseMultMsg": config.advanced.parseMultMsg,
        }
    )


def _build_napcat_config(config: Config) -> NapCatConfig:
    """构造 NapCat 主配置。"""
    return NapCatConfig(
        **{
            "fileLog": config.advanced.fileLog,
            "consoleLog": config.advanced.consoleLog,
            "fileLogLevel": config.advanced.fileLogLevel,
            "consoleLogLevel": config.advanced.consoleLogLevel,
            "packetBackend": config.advanced.packetBackend,
            "packetServer": config.advanced.packetServer,
            "o3HookMode": config.advanced.o3HookMode,
        }
    )


def _read_config_file(strict: bool) -> List[Config]:
    """读取 Bot 配置文件。

    strict=True 时，遇到格式错误或单条配置非法会直接抛错；
    strict=False 时，仅记录错误并返回空列表。
    """
    bot_config_path = _get_path_func().bot_config_path

    try:
        with open(bot_config_path, "r", encoding="utf-8") as file:
            raw_configs = json.load(file)
    except FileNotFoundError:
        return []
    except JSONDecodeError as error:
        if strict:
            raise
        logger.error(f"读取机器人配置失败: {type(error).__name__}: {error}")
        return []

    if not isinstance(raw_configs, list):
        error = TypeError("bot.json 根节点必须为列表")
        if strict:
            raise error
        logger.error(f"读取机器人配置失败: {type(error).__name__}: {error}")
        return []

    configs: list[Config] = []
    for index, raw_config in enumerate(raw_configs):
        try:
            configs.append(Config(**raw_config))
        except Exception as error:
            if strict:
                raise ValueError(f"bot.json 第 {index + 1} 条配置无效") from error
            logger.error(f"读取机器人配置失败: 第 {index + 1} 条配置无效: {type(error).__name__}: {error}")
            return []

    return configs


def _stage_json_write(path: Path, payload: Any) -> Path:
    """将 JSON 数据先写入同目录临时文件。"""
    path.parent.mkdir(parents=True, exist_ok=True)
    temp_path = _next_transaction_path(path, "tmp")

    with open(temp_path, "w", encoding="utf-8") as file:
        json.dump(payload, file, indent=4, ensure_ascii=False)

    return temp_path


def _commit_transaction(replacements: dict[Path, Path], deletions: list[Path]) -> None:
    """提交文件事务，失败时尽量回滚到原始状态。"""
    backups: dict[Path, Path] = {}
    targets = list(replacements.keys()) + [path for path in deletions if path not in replacements]

    try:
        for target in targets:
            if not target.exists():
                continue

            backup_path = _next_transaction_path(target, "bak")
            _replace_path(target, backup_path)
            backups[target] = backup_path

        for target, temp_path in replacements.items():
            _replace_path(temp_path, target)

        for backup_path in backups.values():
            _cleanup_path(backup_path)

    except Exception:
        for target in replacements:
            if target not in backups:
                _cleanup_path(target)

        for target, backup_path in backups.items():
            if not backup_path.exists():
                continue

            try:
                _replace_path(backup_path, target)
            except Exception as restore_error:
                logger.error(f"恢复配置文件失败: {type(restore_error).__name__}: {restore_error}")

        raise


def _apply_json_transaction(payloads: dict[Path, Any], deletions: list[Path] | None = None) -> None:
    """执行 JSON 文件事务。"""
    staged_files: dict[Path, Path] = {}

    try:
        for path, payload in payloads.items():
            staged_files[path] = _stage_json_write(path, payload)

        _commit_transaction(staged_files, deletions or [])
    finally:
        for staged_path in staged_files.values():
            _cleanup_path(staged_path)


def read_config() -> List[Config]:
    """
    ## 读取 NCD 保存的机器人配置文件

    ## 返回
        - List[config] 一个列表, 成员为 config
    """
    return _read_config_file(strict=False)


def write_config(configs: List[Config]) -> None:
    """
    ## 写入 NCD 机器人配置文件
    """
    payload = [_model_to_payload(config) for config in configs]
    _apply_json_transaction({_get_path_func().bot_config_path: payload})


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
        path_func = _get_path_func()
        configs = _read_config_file(strict=True)

        for index, saved_config in enumerate(configs):
            if saved_config.bot.QQID == config.bot.QQID:
                configs[index] = config
                break
        else:
            configs.append(config)

        payloads = {
            path_func.bot_config_path: [_model_to_payload(item) for item in configs],
            path_func.napcat_config_path / f"onebot11_{config.bot.QQID}.json": _model_to_payload(
                _build_onebot_config(config)
            ),
            path_func.napcat_config_path / f"napcat_{config.bot.QQID}.json": _model_to_payload(
                _build_napcat_config(config)
            ),
        }

        _apply_json_transaction(payloads)
        return True

    except (FileNotFoundError, PermissionError, JSONDecodeError, KeyError, TypeError, ValueError, OSError) as error:
        logger.error(f"在写入配置文件时引发 {type(error).__name__}: {error}")
        return False
    except Exception as error:
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
        path_func = _get_path_func()
        configs = _read_config_file(strict=True)

        if not any(saved_config.bot.QQID == config.bot.QQID for saved_config in configs):
            raise ValueError(f"未找到待删除的 Bot 配置: {config.bot.QQID}")

        remaining_configs = [saved_config for saved_config in configs if saved_config.bot.QQID != config.bot.QQID]

        payloads = {
            path_func.bot_config_path: [_model_to_payload(item) for item in remaining_configs],
        }
        deletions = [
            path_func.napcat_config_path / f"onebot11_{config.bot.QQID}.json",
            path_func.napcat_config_path / f"napcat_{config.bot.QQID}.json",
        ]

        _apply_json_transaction(payloads, deletions)
        return True
    except Exception as error:
        logger.error(f"在写入配置文件时引发 {type(error).__name__}: {error}")
        return False
