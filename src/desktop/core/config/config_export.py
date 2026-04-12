# -*- coding: utf-8 -*-
"""当前配置导出服务。"""

# 标准库导入
import json
import zipfile
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path

# 项目内模块导入
import src.desktop.core.config as app_config_module
from src.desktop.core.config import _CURRENT_CONFIG_COMPAT_VERSION, _migrate_config_payload, _write_config_version
from src.desktop.core.config.config_model import BOT_CONFIG_COMPAT_VERSION, serialize_bot_config_collection
from src.desktop.core.config.operate_config import _get_path_func, read_config

_EXPORT_FORMAT_VERSION = "v1"


@dataclass(frozen=True)
class ExportScanResult:
    """扫描当前运行时后得到的可导出结果。"""

    output_dir: Path
    archive_path: Path
    app_config_path: Path | None
    bot_config_path: Path | None
    bot_count: int
    warnings: tuple[str, ...]


@dataclass(frozen=True)
class ExportExecutionPlan:
    """配置导出执行计划。"""

    scan_result: ExportScanResult
    export_app_config: bool = False
    export_bot_config: bool = False


@dataclass(frozen=True)
class ExportExecutionResult:
    """配置导出执行结果。"""

    output_dir: Path
    archive_path: Path
    app_exported: bool
    bot_exported: bool
    exported_bot_count: int
    exported_files: tuple[str, ...]
    warnings: tuple[str, ...]


def _serialize_json(payload: object) -> str:
    return json.dumps(payload, ensure_ascii=False, indent=4)


def _load_current_app_payload(config_path: Path) -> dict[str, object] | None:
    try:
        with open(config_path, "r", encoding="utf-8") as file:
            payload = json.load(file)
    except FileNotFoundError:
        return None

    migrated_payload, config_version, _ = _migrate_config_payload(payload)
    _write_config_version(migrated_payload, config_version)
    return migrated_payload


def _build_export_meta(*, app_exported: bool, bot_exported: bool, exported_bot_count: int) -> dict[str, object]:
    return {
        "exportFormatVersion": _EXPORT_FORMAT_VERSION,
        "appVersion": app_config_module.__version__,
        "exportedAt": datetime.now().astimezone().isoformat(timespec="seconds"),
        "appConfigVersion": _CURRENT_CONFIG_COMPAT_VERSION,
        "botConfigVersion": BOT_CONFIG_COMPAT_VERSION,
        "containsAppConfig": app_exported,
        "containsBotConfig": bot_exported,
        "botCount": exported_bot_count,
    }


def _default_archive_name(now: datetime | None = None) -> str:
    current = now or datetime.now()
    timestamp = (
        f"{current.year}年{current.month}月{current.day}日"
        f"{current.hour:02d}-{current.minute:02d}-{current.second:02d}"
    )
    return f"napcat-config-export-{timestamp}.zip"


def _default_archive_path(output_dir: Path) -> Path:
    return output_dir / _default_archive_name()


def _resolve_archive_path(output_dir: Path) -> Path:
    candidate = _default_archive_path(output_dir)
    index = 1
    while candidate.exists():
        candidate = output_dir / f"{candidate.stem}-{index}{candidate.suffix}"
        index += 1
    return candidate


def scan_current_config_export(target_dir: Path) -> ExportScanResult:
    """扫描当前运行时，确认可导出内容与 ZIP 输出位置。"""

    path_func = _get_path_func()
    resolved_output_dir = Path(target_dir).expanduser().resolve()
    warnings: list[str] = []

    if resolved_output_dir.exists() and not resolved_output_dir.is_dir():
        raise NotADirectoryError(f"导出目标不是文件夹: {resolved_output_dir}")

    archive_path = _resolve_archive_path(resolved_output_dir)
    if archive_path != _default_archive_path(resolved_output_dir):
        warnings.append(f"目标文件夹中已存在同名导出包，将自动写出为: {archive_path.name}")

    app_config_path = None
    try:
        if _load_current_app_payload(path_func.config_path) is not None:
            app_config_path = path_func.config_path
    except Exception as error:
        warnings.append(f"主配置读取失败，已跳过导出候选: {type(error).__name__}")

    bot_configs = read_config()
    bot_config_path = path_func.bot_config_path if bot_configs else None

    if app_config_path is None and bot_config_path is None:
        warnings.append("当前未检测到可导出的主配置或 Bot 配置")

    return ExportScanResult(
        output_dir=resolved_output_dir,
        archive_path=archive_path,
        app_config_path=app_config_path,
        bot_config_path=bot_config_path,
        bot_count=len(bot_configs),
        warnings=tuple(warnings),
    )


def apply_config_export(plan: ExportExecutionPlan) -> ExportExecutionResult:
    """将当前配置打包导出为 ZIP 文件。"""

    output_dir = plan.scan_result.output_dir
    archive_path = plan.scan_result.archive_path
    warnings = list(plan.scan_result.warnings)

    if not plan.export_app_config and not plan.export_bot_config:
        raise ValueError("当前导出计划未选择任何可执行的配置项")

    output_dir.mkdir(parents=True, exist_ok=True)

    archive_members: list[tuple[str, object]] = []
    app_exported = False
    bot_exported = False
    exported_bot_count = 0

    if plan.export_app_config:
        if plan.scan_result.app_config_path is None:
            raise ValueError("当前没有可导出的主配置")
        app_payload = _load_current_app_payload(plan.scan_result.app_config_path)
        if app_payload is None:
            raise ValueError("当前没有可导出的主配置")
        archive_members.append(("config.json", app_payload))
        app_exported = True

    if plan.export_bot_config:
        if plan.scan_result.bot_config_path is None:
            raise ValueError("当前没有可导出的 Bot 配置")
        bot_configs = read_config()
        if not bot_configs:
            raise ValueError("当前没有可导出的 Bot 配置")
        archive_members.append(("bot.json", serialize_bot_config_collection(bot_configs)))
        bot_exported = True
        exported_bot_count = len(bot_configs)

    archive_members.append(
        (
            "export_meta.json",
        _build_export_meta(
            app_exported=app_exported,
            bot_exported=bot_exported,
            exported_bot_count=exported_bot_count,
        ),
        )
    )

    with zipfile.ZipFile(archive_path, mode="w", compression=zipfile.ZIP_DEFLATED) as zip_file:
        for filename, payload in archive_members:
            zip_file.writestr(filename, _serialize_json(payload))

    return ExportExecutionResult(
        output_dir=output_dir,
        archive_path=archive_path,
        app_exported=app_exported,
        bot_exported=bot_exported,
        exported_bot_count=exported_bot_count,
        exported_files=tuple(filename for filename, _ in archive_members),
        warnings=tuple(warnings),
    )
