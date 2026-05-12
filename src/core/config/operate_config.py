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
from src.core.config.config_model import (
    BOT_CONFIG_COMPAT_VERSION,
    RUNTIME_TARGET_LOCAL,
    Config,
    ConfigCollection,
    NapCatConfig,
    OneBotConfig,
    consume_deferred_snowluma_overrides,
    json_payload,
    migrate_bot_config_payload,
    serialize_bot_config_collection,
)
from src.core.logging import logger
from src.core.runtime.paths import PathFunc

_BOT_CONFIG_MIGRATION_BACKUP_SUFFIX = ".bak"
_MISSING = object()


def _get_path_func() -> PathFunc:
    """获取路径处理实例. """
    return it(PathFunc)


def _model_to_payload(model: Config | OneBotConfig | NapCatConfig | ConfigCollection) -> Any:
    """将 Pydantic 模型转换为可序列化 JSON 结构. """
    return json_payload(model)


def _deep_merge_patch(target: dict[str, Any], patch: dict[str, Any]) -> dict[str, Any]:
    """将补丁字典递归合并到目标字典. """
    for key, value in patch.items():
        if isinstance(value, dict) and isinstance(target.get(key), dict):
            _deep_merge_patch(target[key], value)
        else:
            target[key] = value
    return target


def _three_way_merge_full(base: Any, local: Any, remote: Any) -> Any:
    """对完整配置做三方合并, local 冲突优先. """
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
    """按外部补丁范围做三方合并, 返回仅包含补丁键的结果. """
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
    """读取外部派生配置; 格式非法时跳过合并. """
    try:
        payload = _read_json_payload(path)
        return model_type(**payload)
    except FileNotFoundError:
        return None
    except Exception as error:
        logger.warning(f"读取外部配置失败，已跳过合并: path={path}, error={type(error).__name__}: {error}")
        return None


def _build_external_config_patch(path_func: PathFunc, qqid: int) -> dict[str, Any]:
    """从 onebot/napcat 派生配置构造外部补丁. """
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
    """生成同目录下的事务临时路径. """
    return path.with_name(f"{path.name}.{marker}.{uuid.uuid4().hex}")


def _replace_path(src: Path, dst: Path) -> None:
    """替换目标路径, 单独抽出便于测试故障注入. """
    os.replace(src, dst)


def _cleanup_path(path: Path) -> None:
    """清理事务临时文件. """
    try:
        if path.exists():
            path.unlink()
    except FileNotFoundError:
        pass


def _persist_migrated_json(path: Path, payload: Any) -> Path | None:
    """将迁移后的配置原子写回, 并保留一份备份. """
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
    """构造 NapCat OneBot 配置. """
    return OneBotConfig(
        **{
            "network": config.connect,
            "musicSignUrl": config.bot.musicSignUrl,
            "enableLocalFile2Url": config.advanced.enableLocalFile2Url,
            "parseMultMsg": config.advanced.parseMultMsg,
        }
    )


def _build_napcat_config(config: Config) -> NapCatConfig:
    """构造 NapCat 主配置. """
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
    """将 JSON 数据先写入同目录临时文件. """
    path.parent.mkdir(parents=True, exist_ok=True)
    temp_path = _next_transaction_path(path, "tmp")

    with open(temp_path, "w", encoding="utf-8") as file:
        json.dump(payload, file, indent=4, ensure_ascii=False)

    return temp_path


def _commit_transaction(replacements: dict[Path, Path], deletions: list[Path]) -> None:
    """提交文件事务, 失败时尽量回滚到原始状态. """
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
    """执行 JSON 文件事务. """
    staged_files: dict[Path, Path] = {}

    try:
        for path, payload in payloads.items():
            staged_files[path] = _stage_json_write(path, payload)

        _commit_transaction(staged_files, deletions or [])
    finally:
        for staged_path in staged_files.values():
            _cleanup_path(staged_path)


def _read_config_file(strict: bool) -> List[Config]:
    """读取 Bot 配置文件. 

    strict=True 时, 遇到格式错误或单条配置非法会直接抛错; 
    strict=False 时, 仅记录错误并返回空列表. 
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

    # W4 (2026-05-11): 消费 SnowLuma WebUI 密码 override 迁移 deferred 队列.
    # _migrate_legacy_bot_fields 检测到旧 bot.snowluma_webui_password_override 非空时
    # push 到队列; 这里读完 bot.json 后写入 ``cfg.snowluma_webui_password_override``.
    _apply_deferred_snowluma_overrides()

    return collection.bots


def _apply_deferred_snowluma_overrides() -> None:
    """消费 W4 迁移 deferred 队列, 把首个非空值写入 ``cfg.snowluma_webui_password_override``.

    - 队列空: no-op.
    - 队列 = 1 项: 写入并 INFO log.
    - 队列 >= 2 项: 写入首项, WARNING log 提示用户多 Bot 冲突, 其余值被丢弃.
    - cfg 当前已有非空 override: 不覆盖 (优先尊重用户在新版 UI 上设的值).

    Note:
        延迟 import ``cfg``: 测试场景下 ``operate_config`` 可能在没有 ``cfg`` 单例的情况下被
        ``read_config()`` / ``_read_config_file()`` 触达 (例如 model 层 pytest), 此时静默
        跳过写盘. 生产环境主入口 ``main.py`` 已显式构造 ``cfg`` + ``cfg.load()``, 不会落到
        这个分支.
    """
    pending = consume_deferred_snowluma_overrides()
    if not pending:
        return

    try:
        from src.core.config import cfg
    except Exception as exc:  # noqa: BLE001 - 测试 / 极端 import 顺序异常时静默
        logger.warning(
            f"无法 import cfg (W4 迁移失败, 将丢弃 deferred 值 {pending!r}): "
            f"{type(exc).__name__}: {exc}"
        )
        return

    existing = (cfg.get(cfg.snowluma_webui_password_override) or "").strip()
    chosen = pending[0]
    extras = pending[1:]

    if existing:
        logger.info(
            (
                f"W4: cfg.snowluma_webui_password_override 已有非空值, "
                f"丢弃迁移 deferred 值 {pending!r}"
            )
        )
        return

    cfg.set(cfg.snowluma_webui_password_override, chosen, True)
    if extras:
        logger.warning(
            (
                f"W4: 多个 Bot 曾设置不同 SnowLuma 密码 override, 选 '{chosen}' 作为全局值; "
                f"其余将丢弃: {extras!r}"
            )
        )
    else:
        logger.info(
            f"W4: 已迁移 bot.snowluma_webui_password_override 到 "
            f"app.snowluma_webui_password_override (single source value)"
        )


# 2026-05-11 (问题 3 修复, 层 3): 读取 Bot 配置时的合计上限. 与 BotProcessManager.LOCAL_BOT_LIMIT
# 同步, 都是 4 (NTQQ 多开真实限制). 超过时 :func:`read_config` 截断并把被隐藏的 QQID 推入
# deferred queue, UI 层在合适时机 (例如 BotListPage.update_bot_list) 消费.
_LOCAL_BOT_READ_LIMIT: int = 4

# Deferred queue: 收集被 read_config 截断的 QQID 字符串, 让 UI 层消费一次后清空.
# UI 通过 :func:`consume_truncated_bots_warning` 拿到并展示 warning_bar, 避免重复弹.
_truncated_bots_pending: list[str] = []


def consume_truncated_bots_warning() -> list[str]:
    """消费 deferred 队列: 取出上一次 :func:`read_config` 截断的 QQID 列表, 清空队列.

    UI 调用方 (一般是 :class:`BotListPage.update_bot_list`) 拿到后用 warning_bar 提示用户.
    若多次调 :func:`read_config` 但 UI 没消费, 队列会累积; 但 UI 每次消费后清零,
    所以"展示一次后不再骚扰"的语义自然成立.

    Returns:
        被隐藏的 QQID 字符串列表; 队列空时返回 ``[]``.
    """
    global _truncated_bots_pending
    result = _truncated_bots_pending
    _truncated_bots_pending = []
    return result


def read_config_raw() -> List[Config]:
    """读取 NCD 保存的机器人配置文件, **不应用 4 个上限截断**.

    专供下列场景:

    - 写盘前的合并 (``update_config`` 走 ``_read_config_file(strict=True)``);
    - 添加 Bot 时的总数检查 (``slot_save_config_button``) — 真实总数才能让用户
      看到 "5 个 Bot 中已达上限" 提示, 而不是截断后的 "4 个 / 4 个" 没意义计数;
    - 测试 / 诊断脚本.

    UI 列表展示请用 :func:`read_config` (会截断 + push deferred queue).

    Returns:
        完整 Bot 配置列表 (可能 > 4 个).
    """
    return _read_config_file(strict=False)


def read_config() -> List[Config]:
    """
    ## 读取 NCD 保存的机器人配置文件 (UI 用; 截断到 4 个)

    2026-05-11 (问题 3 修复, 层 3): NTQQ 多开真实上限 4 个, 与后端类型无关.
    本函数读取 ``bot.json`` 后**仅返回前 4 个**, 超出部分:

    - 记录 ``logger.warning`` 含被隐藏的 QQID 列表 (脱敏前完整 QQID, 留诊断用);
    - 推入 :func:`consume_truncated_bots_warning` 的 deferred queue, UI 层下一次消费;
    - **不写回**, 避免破坏用户原始数据. 用户主动删除多余 Bot 后才会真正落盘.

    用途区分:

    - 本函数: UI 列表展示 / 启动按钮枚举 / 卡片渲染 (用户视角).
    - :func:`read_config_raw`: 写盘合并 / 总数检查 / 测试 (内部视角, 拿完整列表).

    ## 返回
        - List[config] 一个列表, 成员为 config (最多 4 个)
    """
    configs = _read_config_file(strict=False)
    if len(configs) <= _LOCAL_BOT_READ_LIMIT:
        return configs

    extras = configs[_LOCAL_BOT_READ_LIMIT:]
    extras_qqids = [str(c.bot.QQID) for c in extras]
    logger.warning(
        (
            f"读取 Bot 配置截断 (NTQQ 多开 {_LOCAL_BOT_READ_LIMIT} 个上限): "
            f"共 {len(configs)} 个, 隐藏后 {len(extras)} 个 QQID={extras_qqids}; "
            "如需调整, 请删除现有 Bot 后再添加"
        )
    )
    _truncated_bots_pending.extend(extras_qqids)
    return configs[:_LOCAL_BOT_READ_LIMIT]


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
    """将当前编辑结果与磁盘配置, WebUI 派生配置做无感合并. """
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


def _build_per_bot_payloads(path_func: PathFunc, config_to_save: Config) -> dict[Path, Any]:
    """按 ``backend_type`` 返回当前 Bot 应写入的 per-bot 派生配置文件 payload.

    - **NAPCAT** (默认, 与历史行为一致): 写入 ``onebot11_<qq>.json`` + ``napcat_<qq>.json``
      到 :attr:`PathFunc.napcat_config_path`.
    - **SNOWLUMA**: 写入 ``onebot_<qq>.json`` 到 :meth:`PathFunc.get_snowluma_config_dir`
      (与 :func:`render_onebot_json` 写盘路径一致, 与 SnowLuma driver / 远端 SFTP 同源).
      远端 SL Bot 也会同时写本地一份, 让 Bot 迁移 (``BotMigrationService``) 能像 NC
      一样从本地 ``<snowluma>/config/onebot_<uin>.json`` 直接拷过去, 与
      :meth:`RemoteSnowLumaBackend.write_bot_runtime_config` 对称.

    Returns:
        ``{Path -> json-serializable dict}`` 字典, 调用方再合并 ``bot.json`` 后丢给
        :func:`_apply_json_transaction`.
    """
    # 延迟导入避免循环 (runtime 模块依赖 config_model)
    from src.core.runtime.backend_type import BackendType

    qq_id = config_to_save.bot.QQID
    if config_to_save.bot.backend_type == BackendType.SNOWLUMA:
        from src.core.runtime.snowluma_config_renderer import build_onebot_payload

        sl_payload = build_onebot_payload(
            int(qq_id),
            connect=config_to_save.connect,
            music_sign_url=config_to_save.bot.musicSignUrl,
        )
        return {
            path_func.get_snowluma_config_dir() / f"onebot_{qq_id}.json": sl_payload,
        }

    return {
        path_func.napcat_config_path / f"onebot11_{qq_id}.json": _model_to_payload(
            _build_onebot_config(config_to_save)
        ),
        path_func.napcat_config_path / f"napcat_{qq_id}.json": _model_to_payload(
            _build_napcat_config(config_to_save)
        ),
    }


def _list_per_bot_files(path_func: PathFunc, config: Config) -> list[Path]:
    """按 ``backend_type`` 列出待清理的 per-bot 派生配置文件路径.

    与 :func:`_build_per_bot_payloads` 对称: NAPCAT 删 onebot11/napcat 两个,
    SNOWLUMA 删 ``onebot_<uin>.json`` 一个.
    """
    from src.core.runtime.backend_type import BackendType

    qq_id = config.bot.QQID
    if config.bot.backend_type == BackendType.SNOWLUMA:
        return [path_func.get_snowluma_config_dir() / f"onebot_{qq_id}.json"]
    return [
        path_func.napcat_config_path / f"onebot11_{qq_id}.json",
        path_func.napcat_config_path / f"napcat_{qq_id}.json",
    ]


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

        payloads: dict[Path, Any] = {
            path_func.bot_config_path: serialize_bot_config_collection(configs),
        }
        payloads.update(_build_per_bot_payloads(path_func, config_to_save))

        _apply_json_transaction(payloads)

        # P2.4: 若 Bot 绑定了远端服务器, 同步把 NC / SL 派生配置 (按 backend_type) 推到远端工作区.
        # 同步失败仅记录 warning, 不影响本地保存的成功语义 (用户至少能在本地看到配置).
        _sync_bot_runtime_config_to_remote(config_to_save)
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
        deletions = _list_per_bot_files(path_func, config)

        _apply_json_transaction(payloads, deletions)

        # P2.4: 若 Bot 此前绑定到远端服务器, 同步清理远端配置文件 (按 backend_type 分发)
        _delete_bot_runtime_config_from_remote(config)
        return True
    except Exception as error:
        logger.error(f"在写入配置文件时引发 {type(error).__name__}: {error}")
        return False


# ==================== P2.4: 远端配置同步钩子 ====================
def _do_remote_sync_blocking(config: Config) -> None:
    """**同步**把 Bot 配置推到远端 (P3 perf W3 拆分点).

    把原 :func:`_sync_bot_runtime_config_to_remote` 的核心 SSH 工作抽出来,
    供同步路径 (无 Qt 上下文 / 测试) 与异步路径 (Qt UI 线程派发到 QThreadPool) 共用.

    设计要点 (与历史一致):

    - 解析失败 (server_id 不存在 / SSH 不通) 不应让本地保存返回 False;
      用户应当看到本地保存成功, 然后由 UI 后续展示远端同步状态.
    - **顶层 try/except Exception 兜底**: 任何未预期异常 (paramiko/keyring/...)
      都被吞下转 warning, 绝不让远端同步副作用使本地的 ``bot.json`` 写盘看起来失败.
      这是 P3 阶段修复的实测 bug -- 远端 SSH 抖动会让 update_config 返回 False,
      用户感知"切了 runtime_target 完全没生效", 但其实本地早就写盘了.
    """
    try:
        try:
            # 延迟导入避免循环依赖 (operation -> remote -> ssh, 在测试环境下
            # 不一定都装齐了).
            from src.core.operation.resolver import (
                BackendResolutionError,
                resolve_backend_for_bot,
            )

            backend = resolve_backend_for_bot(config)
        except BackendResolutionError as exc:
            logger.warning(
                f"远端配置同步跳过: 解析 backend 失败 (QQID={config.bot.QQID}, "
                f"target={config.bot.runtime_target}, stage={exc.stage}): {exc}"
            )
            return
        except ImportError as exc:
            logger.warning(f"远端配置同步跳过: backend 模块不可用: {exc}")
            return

        # 仅 RemoteBackend 才有 ``write_bot_runtime_config`` 方法; LocalBackend 不应被绑定到非 local target.
        write_remote = getattr(backend, "write_bot_runtime_config", None)
        if write_remote is None:
            logger.warning(
                f"远端配置同步跳过: 解析得到的 backend ({type(backend).__name__}) 不支持远端配置写入"
            )
            return

        onebot_path, napcat_path = write_remote(config)
        logger.info(
            (
                "远端 Bot 配置同步完成: "
                f"QQID={config.bot.QQID}, target={config.bot.runtime_target}, "
                f"onebot={onebot_path}, napcat={napcat_path}"
            )
        )
    except Exception as exc:  # noqa: BLE001 - 远端同步失败绝不应让本地保存失败
        logger.warning(
            f"远端配置同步失败 (QQID={config.bot.QQID}, "
            f"target={config.bot.runtime_target}): {type(exc).__name__}: {exc}"
        )


def _do_remote_delete_blocking(config: Config) -> None:
    """**同步**删除远端 Bot 配置 (P3 perf W3 拆分点).

    与 :func:`_do_remote_sync_blocking` 对称的同步实现, 供同步与异步两条路径共享.
    """
    try:
        try:
            from src.core.operation.resolver import (
                BackendResolutionError,
                resolve_backend_for_bot,
            )

            backend = resolve_backend_for_bot(config)
        except BackendResolutionError as exc:
            logger.warning(
                f"远端配置删除跳过: 解析 backend 失败 (QQID={config.bot.QQID}, "
                f"target={config.bot.runtime_target}, stage={exc.stage}): {exc}"
            )
            return
        except ImportError as exc:
            logger.warning(f"远端配置删除跳过: backend 模块不可用: {exc}")
            return

        delete_remote = getattr(backend, "delete_bot_runtime_config", None)
        if delete_remote is None:
            return

        delete_remote(str(config.bot.QQID))
        logger.info(
            f"远端 Bot 配置已删除: QQID={config.bot.QQID}, target={config.bot.runtime_target}"
        )
    except Exception as exc:  # noqa: BLE001 - 远端清理失败不应让本地删除失败
        logger.warning(
            f"远端配置删除失败 (QQID={config.bot.QQID}, "
            f"target={config.bot.runtime_target}): {type(exc).__name__}: {exc}"
        )


# ==================== P3 perf W3: 异步派发开关 ====================
# 测试钩子: 单测可通过 monkeypatch / fixture 把它设为 True, 强制走同步路径,
# 让"派发完立刻断言 spy 调用"的旧 case 仍能在共享 QApp 的 pytest 进程里通过.
# 生产代码不应触碰它.
_FORCE_SYNC_REMOTE_CONFIG = False


def _try_dispatch_remote_op_async(action: str, config: Config) -> bool:
    """P3 perf W3: 把远端配置同步 / 删除推到 QThreadPool 后台线程, 不阻塞 UI.

    Args:
        action: ``"sync"`` 或 ``"delete"``, 决定 runnable 内部走哪条同步函数.
        config: 上下文 [`Config`](src/core/config/config_model.py).

    Returns:
        - ``True``: 已成功派发到 QThreadPool, 调用方应直接返回, 不再走同步路径.
        - ``False``: 当前不在 Qt 上下文 (没有 QApplication 实例) / 派发失败 /
          测试钩子 :data:`_FORCE_SYNC_REMOTE_CONFIG` 为 True, 调用方应回退到同步执行.

    设计取舍:

    - 检查 ``QApplication.instance()`` 是判断"是否在 UI 进程内运行"的最可靠信号.
      在 ``test_operate_config.py`` 这类无 QApp 的纯 logic test 中, 我们必然走同步,
      保留向下兼容; 在真正的 UI 进程里, 走异步避免"保存配置卡 N 秒"的体感.
    - :data:`_FORCE_SYNC_REMOTE_CONFIG` 用于解决"同进程跨 case 残留 QApp"的污染:
      pytest 一次进程跑很多 UI 用例时, 第一个创建的 QApp 不会被销毁, 之后的同步
      期望 case 会误入异步分支; 显式 toggle 让旧 case 不必感知这个细节.
    - 任何异常 (Qt 模块缺失 / pool 无法初始化 / runnable 构造失败) 一律回退同步,
      避免新代码路径带来的 regression 把保存功能整体破坏.
    """
    if _FORCE_SYNC_REMOTE_CONFIG:
        return False

    try:
        from PySide6.QtWidgets import QApplication
    except Exception:  # noqa: BLE001
        return False

    if QApplication.instance() is None:
        return False

    try:
        # P3 perf W4: 远端配置 SFTP 写 / SSH rm 也走 remote_ssh_pool, 与 QThreadPool
        # 全局池隔离, 避免与本地头像下载 / 版本探测等短任务互抢线程.
        from src.core.remote.thread_pool import remote_ssh_pool

        runnable = _RemoteConfigOpRunnable(action=action, config=config)
        remote_ssh_pool().start(runnable)
        return True
    except Exception as exc:  # noqa: BLE001 - 派发失败回退同步, 不破坏保存语义
        logger.warning(
            f"远端配置 {action} 异步派发失败, 回退同步执行 (QQID={config.bot.QQID}): "
            f"{type(exc).__name__}: {exc}"
        )
        return False


def _sync_bot_runtime_config_to_remote(config: Config) -> None:
    """如果 Bot 绑定到远端, 把 onebot11/napcat JSON 推到远端工作区.

    P3 perf W3: 调用方不再阻塞在 SSH 写文件上 - 在 UI 进程内会派发到 QThreadPool;
    在无 Qt 上下文的环境 (单测 / CLI) 仍走同步, 维持兼容. 失败时仅记录 warning, 不抛出.

    要求:

    - 调用方应在本地 ``_apply_json_transaction`` 提交成功**之后**才触发同步,
      以避免本地写盘失败时把陈旧配置推到远端.
    """
    if config.bot.runtime_target == RUNTIME_TARGET_LOCAL:
        return

    if _try_dispatch_remote_op_async("sync", config):
        return

    _do_remote_sync_blocking(config)


def _delete_bot_runtime_config_from_remote(config: Config) -> None:
    """删除远端的 onebot11/napcat 配置文件; 仅在 Bot 绑定到远端时触发.

    P3 perf W3: 与 :func:`_sync_bot_runtime_config_to_remote` 对称, UI 进程内派发到
    QThreadPool 后台线程, 无 Qt 上下文时回退同步.
    """
    if config.bot.runtime_target == RUNTIME_TARGET_LOCAL:
        return

    if _try_dispatch_remote_op_async("delete", config):
        return

    _do_remote_delete_blocking(config)


# ==================== P3 perf W3: 异步 runnable ====================
def _import_runnable_base():
    """延迟导入 ``QRunnable`` 与 ``QObject`` 基类.

    分离出来便于测试时 monkeypatch; 同时把 PySide6 重型导入限制在 UI 上下文.
    """
    from PySide6.QtCore import QObject, QRunnable

    return QObject, QRunnable


def _make_runnable_class():
    """构造 ``_RemoteConfigOpRunnable`` 类型 (惰性, 仅在 UI 进程触发).

    把它写成"按需构造", 避免 PySide6 在 import operate_config 时被强制加载,
    保持纯 logic test (test_operate_config.py) 与无 Qt 头依赖的 CI 路径正常.
    """
    QObject, QRunnable = _import_runnable_base()

    class _RemoteConfigOpRunnable(QObject, QRunnable):
        """P3 perf W3: 在 QThreadPool 上跑远端配置同步 / 删除, 顺带挂到
        [`BackgroundTaskCenter`](src/core/runtime/background_tasks.py).
        """

        _ACTION_LABELS = {
            "sync": "同步配置到远端 Bot {qq_id}",
            "delete": "删除远端 Bot {qq_id} 的配置",
        }
        _ACTION_CONTENTS = {
            "sync": "正在通过 SFTP 推送 onebot11 / napcat JSON...",
            "delete": "正在通过 SSH 删除远端配置...",
        }
        _ACTION_SUCCESS = {
            "sync": "Bot {qq_id} 配置已同步",
            "delete": "Bot {qq_id} 远端配置已清理",
        }

        def __init__(self, *, action: str, config: Config) -> None:
            QObject.__init__(self)
            QRunnable.__init__(self)
            self._action = action
            self._config = config
            self.setAutoDelete(True)

        def run(self) -> None:  # noqa: D401 - QRunnable 框架约定
            qq_id = str(self._config.bot.QQID)
            task_id = f"remote-config-{self._action}-{qq_id}"
            label = self._ACTION_LABELS.get(self._action, "远端配置操作 {qq_id}").format(qq_id=qq_id)
            content = self._ACTION_CONTENTS.get(self._action, "")

            center = None
            try:
                from creart import it as _it
                from src.core.runtime.background_tasks import BackgroundTaskCenter

                center = _it(BackgroundTaskCenter)
                center.begin(task_id, label, content=content)
            except Exception:  # noqa: BLE001 - center 不可用时仍执行 SSH 写, 不影响主流程
                center = None

            success = False
            failure_message = ""
            try:
                if self._action == "sync":
                    _do_remote_sync_blocking(self._config)
                    success = True
                elif self._action == "delete":
                    _do_remote_delete_blocking(self._config)
                    success = True
                else:
                    failure_message = f"未知远端配置操作: action={self._action}"
                    logger.warning(f"{failure_message}, QQID={qq_id}")
            except Exception as exc:  # noqa: BLE001 - 兜底; blocking 函数本身已 try/except
                failure_message = f"{type(exc).__name__}: {exc}"
                logger.warning(
                    f"远端配置 {self._action} runnable 抛异常 (QQID={qq_id}): {failure_message}"
                )
            finally:
                if center is not None:
                    try:
                        if success:
                            success_msg = self._ACTION_SUCCESS.get(self._action, "").format(
                                qq_id=qq_id
                            )
                            center.end(task_id, success=True, message=success_msg)
                        else:
                            center.fail(
                                task_id, failure_message or f"远端配置 {self._action} 失败"
                            )
                    except Exception:  # noqa: BLE001
                        pass

    return _RemoteConfigOpRunnable


def _RemoteConfigOpRunnable(*, action: str, config: Config):  # noqa: N802 - 保持类语义的工厂签名
    """对外暴露的 runnable 构造点. 被 :func:`_try_dispatch_remote_op_async` 与单测复用.

    内部走 :func:`_make_runnable_class` 惰性构造, 这样 ``import operate_config`` 不必拉
    PySide6 模块树.
    """
    cls = _make_runnable_class()
    return cls(action=action, config=config)
