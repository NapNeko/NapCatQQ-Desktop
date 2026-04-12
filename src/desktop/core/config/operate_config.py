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
from src.desktop.core.config.config_model import (
    BOT_CONFIG_COMPAT_VERSION,
    Config,
    ConfigCollection,
    NapCatConfig,
    OneBotConfig,
    json_payload,
    migrate_bot_config_payload,
    serialize_bot_config_collection,
)
from src.desktop.core.logging import logger
from src.desktop.core.runtime.paths import PathFunc

_BOT_CONFIG_MIGRATION_BACKUP_SUFFIX = ".bak"
_MISSING = object()


def _get_path_func() -> PathFunc:
    """获取路径处理实例。"""
    return it(PathFunc)


def _model_to_payload(model: Config | OneBotConfig | NapCatConfig | ConfigCollection) -> Any:
    """将 Pydantic 模型转换为可序列化 JSON 结构。"""
    return json_payload(model)


def _deep_merge_patch(target: dict[str, Any], patch: dict[str, Any]) -> dict[str, Any]:
    """将补丁字典递归合并到目标字典。"""
    for key, value in patch.items():
        if isinstance(value, dict) and isinstance(target.get(key), dict):
            _deep_merge_patch(target[key], value)
        else:
            target[key] = value
    return target


def _three_way_merge_full(base: Any, local: Any, remote: Any) -> Any:
    """对完整配置做三方合并，local 冲突优先。"""
    if isinstance(base, dict) and isinstance(local, dict) and isinstance(remote, dict):
        merged: dict[str, Any] = {}
        for key in set(base) | set(local) | set(remote):
            base_value = base.get(key, _MISSING)
            local_value = local.get(key, _MISSING)
            remote_value = remote.get(key, _MISSING)

            if base_value is _MISSING:
                merged[key] = remote_value if local_value is _MISSING else local_value
                continue
            if local_value is _MISSING:
                merged[key] = remote_value if remote_value is not _MISSING else base_value
                continue
            if remote_value is _MISSING:
                merged[key] = local_value
                continue

            merged[key] = _three_way_merge_full(base_value, local_value, remote_value)
        return merged

    if local == base:
        return remote
    if remote == base or local == remote:
        return local
    return local


def _merge_external_patch(base: Any, local: Any, external_patch: Any) -> Any:
    """按外部补丁范围做三方合并，返回仅包含补丁键的结果。"""
    if isinstance(external_patch, dict):
        merged: dict[str, Any] = {}
        base_dict = base if isinstance(base, dict) else {}
        local_dict = local if isinstance(local, dict) else {}
        for key, value in external_patch.items():
            merged[key] = _merge_external_patch(base_dict.get(key, _MISSING), local_dict.get(key, _MISSING), value)
        return merged

    if local is _MISSING or local == base:
        return external_patch
    if external_patch == base or local == external_patch:
        return local
    return local


def _read_json_payload(path: Path) -> object:
    with open(path, "r", encoding="utf-8") as file:
        return json.load(file)


def _load_external_model(path: Path, model_type: type[OneBotConfig] | type[NapCatConfig]) -> OneBotConfig | NapCatConfig | None:
    """读取外部派生配置；格式非法时跳过合并。"""
    try:
        payload = _read_json_payload(path)
        return model_type(**payload)
    except FileNotFoundError:
        return None
    except Exception as error:
        logger.warning(f"读取外部配置失败，已跳过合并: path={path}, error={type(error).__name__}: {error}")
        return None


def _build_external_config_patch(path_func: PathFunc, qqid: int) -> dict[str, Any]:
    """从 onebot/napcat 派生配置构造外部补丁。"""
    patch: dict[str, Any] = {}
    onebot_path = path_func.napcat_config_path / f"onebot11_{qqid}.json"
    napcat_path = path_func.napcat_config_path / f"napcat_{qqid}.json"

    if isinstance(onebot_config := _load_external_model(onebot_path, OneBotConfig), OneBotConfig):
        _deep_merge_patch(
            patch,
            {
                "bot": {"musicSignUrl": onebot_config.musicSignUrl},
                "connect": _model_to_payload(onebot_config.network),
                "advanced": {
                    "enableLocalFile2Url": onebot_config.enableLocalFile2Url,
                    "parseMultMsg": onebot_config.parseMultMsg,
                },
            },
        )

    if isinstance(napcat_config := _load_external_model(napcat_path, NapCatConfig), NapCatConfig):
        _deep_merge_patch(
            patch,
            {
                "advanced": {
                    "fileLog": napcat_config.fileLog,
                    "consoleLog": napcat_config.consoleLog,
                    "fileLogLevel": napcat_config.fileLogLevel,
                    "consoleLogLevel": napcat_config.consoleLogLevel,
                    "packetBackend": napcat_config.packetBackend,
                    "packetServer": napcat_config.packetServer,
                    "o3HookMode": napcat_config.o3HookMode,
                    "bypass": _model_to_payload(napcat_config.bypass),
                }
            },
        )

    return patch


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


def _persist_migrated_json(path: Path, payload: Any) -> Path | None:
    """将迁移后的配置原子写回，并保留一份备份。"""
    path.parent.mkdir(parents=True, exist_ok=True)
    temp_path = _next_transaction_path(path, "tmp")
    backup_path = path.with_name(f"{path.name}{_BOT_CONFIG_MIGRATION_BACKUP_SUFFIX}")
    backup_created = False

    try:
        with open(temp_path, "w", encoding="utf-8") as file:
            json.dump(payload, file, indent=4, ensure_ascii=False)

        if path.exists() and not backup_path.exists():
            _replace_path(path, backup_path)
            backup_created = True

        _replace_path(temp_path, path)
        return backup_path if backup_created else None
    except Exception:
        _cleanup_path(temp_path)
        if backup_created and backup_path.exists() and not path.exists():
            try:
                _replace_path(backup_path, path)
            except Exception as restore_error:
                logger.error(f"恢复 bot 配置文件失败: {type(restore_error).__name__}: {restore_error}")
        raise


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
            "bypass": config.advanced.bypass,
        }
    )


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


def _read_config_file(strict: bool) -> List[Config]:
    """读取 Bot 配置文件。

    strict=True 时，遇到格式错误或单条配置非法会直接抛错；
    strict=False 时，仅记录错误并返回空列表。
    """
    bot_config_path = _get_path_func().bot_config_path

    try:
        with open(bot_config_path, "r", encoding="utf-8") as file:
            raw_payload = json.load(file)
    except FileNotFoundError:
        return []
    except JSONDecodeError as error:
        if strict:
            raise
        logger.error(f"读取机器人配置失败: {type(error).__name__}: {error}")
        return []

    try:
        migrated_payload, source_version, migration_rules = migrate_bot_config_payload(raw_payload)
        collection = ConfigCollection(**migrated_payload)
    except Exception as error:
        if strict:
            raise
        logger.error(f"读取机器人配置失败: {type(error).__name__}: {error}")
        return []

    if raw_payload != migrated_payload:
        try:
            backup_path = _persist_migrated_json(bot_config_path, migrated_payload)
        except Exception as migration_error:
            logger.error(
                (
                    "bot 配置迁移写回失败，将继续使用内存中的迁移结果: "
                    f"path={bot_config_path}, source_version={source_version}, "
                    f"target_version={BOT_CONFIG_COMPAT_VERSION}, "
                    f"rules={migration_rules}, error={type(migration_error).__name__}: {migration_error}"
                )
            )
        else:
            logger.info(
                (
                    "bot 配置迁移完成: "
                    f"path={bot_config_path}, source_version={source_version}, "
                    f"target_version={BOT_CONFIG_COMPAT_VERSION}, "
                    f"rules={migration_rules}, backup={backup_path if backup_path else 'existing-or-skipped'}"
                )
            )

    return collection.bots


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
    payload = serialize_bot_config_collection(configs)
    _apply_json_transaction({_get_path_func().bot_config_path: payload})


def check_duplicate_bot(config: Config) -> bool:
    """
    ## 检查是否已存在相同的机器人配置

    ## 参数
         - config 传入的机器人配置

    ## 返回
         - bool 类型
    """
    configs: List[Config]

    for bot_config in read_config():
        if config.bot.QQID == bot_config.bot.QQID:
            return True
    return False


def merge_config_for_update(config: Config, base_config: Config | None = None) -> Config:
    """将当前编辑结果与磁盘配置、WebUI 派生配置做无感合并。"""
    path_func = _get_path_func()
    current_configs = _read_config_file(strict=True)
    current_saved_config = next((item for item in current_configs if item.bot.QQID == config.bot.QQID), None)

    if base_config is None:
        base_config = current_saved_config

    if base_config is None:
        return config

    base_payload = _model_to_payload(base_config)
    local_payload = _model_to_payload(config)
    merged_payload = local_payload

    if current_saved_config is not None:
        current_payload = _model_to_payload(current_saved_config)
        merged_payload = _three_way_merge_full(base_payload, local_payload, current_payload)

    external_patch = _build_external_config_patch(path_func, int(config.bot.QQID))
    if external_patch:
        merged_external_patch = _merge_external_patch(base_payload, local_payload, external_patch)
        merged_payload = _deep_merge_patch(merged_payload, merged_external_patch)

    return Config(**merged_payload)


def update_config(config: Config, base_config: Config | None = None, *, skip_merge: bool = False) -> bool:
    """
    ## 更新配置到配置文件
    """
    try:
        path_func = _get_path_func()
        config_to_save = config if skip_merge else merge_config_for_update(config, base_config=base_config)
        configs = _read_config_file(strict=True)

        for index, saved_config in enumerate(configs):
            if saved_config.bot.QQID == config_to_save.bot.QQID:
                configs[index] = config_to_save
                break
        else:
            configs.append(config_to_save)

        payloads = {
            path_func.bot_config_path: serialize_bot_config_collection(configs),
            path_func.napcat_config_path / f"onebot11_{config_to_save.bot.QQID}.json": _model_to_payload(
                _build_onebot_config(config_to_save)
            ),
            path_func.napcat_config_path / f"napcat_{config_to_save.bot.QQID}.json": _model_to_payload(
                _build_napcat_config(config_to_save)
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
    """
    try:
        path_func = _get_path_func()
        configs = _read_config_file(strict=True)

        if not any(saved_config.bot.QQID == config.bot.QQID for saved_config in configs):
            raise ValueError(f"未找到待删除的 Bot 配置: {config.bot.QQID}")

        remaining_configs = [saved_config for saved_config in configs if saved_config.bot.QQID != config.bot.QQID]

        payloads = {
            path_func.bot_config_path: serialize_bot_config_collection(remaining_configs),
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
