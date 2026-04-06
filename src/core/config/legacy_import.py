# -*- coding: utf-8 -*-
"""旧版配置目录导入服务。

文件名识别规则以历史兼容性为主：
- ``config.json``
- ``bot.json``
- ``onebot11_{qqid}.json``
- ``napcat_{qqid}.json``

根据当前仓库抽查到的 git 历史，这些文件名长期保持稳定，变化主要发生在目录布局，
因此扫描策略采用“固定文件名 + 已知布局 + 内容校验”。
"""

# 标准库导入
import json
import shutil
import zipfile
from dataclasses import dataclass, field
from datetime import datetime
from json import JSONDecodeError
from pathlib import Path
from typing import Literal

# 项目内模块导入
from src.core.config import (
    _CURRENT_CONFIG_COMPAT_VERSION,
    _migrate_config_payload,
    _write_config_version,
)
from src.core.config.config_model import (
    BOT_CONFIG_COMPAT_VERSION,
    Config,
    ConfigCollection,
    migrate_bot_config_payload,
    serialize_bot_config_collection,
)
from src.core.config.operate_config import (
    _apply_json_transaction,
    _build_napcat_config,
    _build_onebot_config,
    _get_path_func,
    _model_to_payload,
    _read_config_file,
)
from src.core.logging import logger
from src.core.runtime.paths import PathFunc

_APP_CONFIG_GROUP_KEYS = {
    "Info",
    "Personalize",
    "Personalized",
    "General",
    "HideTips",
    "QFluentWidgets",
    "Home",
    "Event",
    "Email",
    "WebHook",
}
_AUXILIARY_GLOB_PATTERNS = ("onebot11_*.json", "napcat_*.json")
_MAX_IMPORT_ARCHIVE_SIZE_BYTES = 50 * 1024 * 1024


@dataclass(frozen=True)
class ImportConflictItem:
    """Bot 导入冲突项。"""

    qqid: int
    current_name: str
    imported_name: str


@dataclass(frozen=True)
class ImportScanResult:
    """扫描旧版配置目录后的结果。"""

    source_path: Path
    scan_root_path: Path
    source_kind: Literal["directory", "zip"]
    app_config_path: Path | None
    bot_config_path: Path | None
    auxiliary_paths: tuple[Path, ...]
    imported_bot_count: int
    conflicts: tuple[ImportConflictItem, ...]
    warnings: tuple[str, ...]
    cleanup_path: Path | None = None

    @property
    def root_path(self) -> Path:
        """兼容旧字段名，返回实际扫描根目录。"""

        return self.scan_root_path


@dataclass(frozen=True)
class ImportExecutionPlan:
    """导入执行计划。"""

    scan_result: ImportScanResult
    import_app_config: bool = False
    bot_import_mode: Literal["skip", "replace", "append"] = "skip"
    overwrite_conflict_qqids: frozenset[int] = field(default_factory=frozenset)


@dataclass(frozen=True)
class ImportExecutionResult:
    """导入执行结果。"""

    app_imported: bool
    bot_imported: bool
    imported_bot_count: int
    replaced_bot_count: int
    appended_bot_count: int
    skipped_bot_count: int
    backup_dir: Path
    warnings: tuple[str, ...]


def _read_json_file(path: Path) -> object:
    with open(path, "r", encoding="utf-8") as file:
        return json.load(file)


def _is_auxiliary_config_path(path: Path) -> bool:
    return path.name.startswith("onebot11_") or path.name.startswith("napcat_")


def _candidate_score(path: Path, sibling_bot: bool = False, sibling_app: bool = False) -> int:
    parts = [part.lower() for part in path.parts]
    score = 0
    if path.parent.name.lower() == "config":
        score += 4
    if "runtime" in parts:
        score += 3
    if sibling_bot or sibling_app:
        score += 2
    if _is_auxiliary_config_path(path):
        score -= 100
    if "versions" in parts:
        score -= 10
    return score


def _looks_like_app_config_payload(payload: object) -> bool:
    if not isinstance(payload, dict):
        return False
    return bool(_APP_CONFIG_GROUP_KEYS.intersection(payload.keys()))


def _load_bot_configs(path: Path) -> list[Config]:
    migrated_payload, _, _ = migrate_bot_config_payload(_read_json_file(path))
    return ConfigCollection(**migrated_payload).bots


def _scan_app_candidate(path: Path) -> tuple[int, bool]:
    try:
        payload = _read_json_file(path)
    except (FileNotFoundError, JSONDecodeError, OSError):
        return -100, False

    if not _looks_like_app_config_payload(payload):
        return -100, False

    sibling_bot = (path.parent / "bot.json").exists()
    sibling_app = False
    return 10 + _candidate_score(path, sibling_bot=sibling_bot, sibling_app=sibling_app), True


def _scan_bot_candidate(path: Path) -> tuple[int, list[Config] | None]:
    try:
        configs = _load_bot_configs(path)
    except Exception:
        return -100, None

    sibling_app = (path.parent / "config.json").exists()
    return 10 + _candidate_score(path, sibling_bot=False, sibling_app=sibling_app), configs


def _iter_candidates(folder: Path, filename: str) -> list[Path]:
    return sorted({path for path in folder.rglob(filename) if path.is_file()})


def _select_best_app_config(folder: Path, warnings: list[str]) -> Path | None:
    candidates: list[tuple[int, Path]] = []
    for candidate in _iter_candidates(folder, "config.json"):
        score, is_valid = _scan_app_candidate(candidate)
        if is_valid:
            candidates.append((score, candidate))

    if not candidates:
        return None

    candidates.sort(key=lambda item: (-item[0], len(item[1].parts), str(item[1]).lower()))
    if len(candidates) > 1:
        warnings.append("检测到多个 config.json 候选，已按目录布局和内容校验选择最可信的一项")
    return candidates[0][1]


def _select_best_bot_config(folder: Path, warnings: list[str]) -> tuple[Path | None, list[Config]]:
    candidates: list[tuple[int, Path, list[Config]]] = []
    for candidate in _iter_candidates(folder, "bot.json"):
        score, configs = _scan_bot_candidate(candidate)
        if configs is not None:
            candidates.append((score, candidate, configs))

    if not candidates:
        return None, []

    candidates.sort(key=lambda item: (-item[0], len(item[1].parts), str(item[1]).lower()))
    if len(candidates) > 1:
        warnings.append("检测到多个 bot.json 候选，已按目录布局和内容校验选择最可信的一项")
    return candidates[0][1], candidates[0][2]


def _collect_auxiliary_paths(folder: Path) -> tuple[Path, ...]:
    auxiliary_paths: set[Path] = set()
    for pattern in _AUXILIARY_GLOB_PATTERNS:
        auxiliary_paths.update(path for path in folder.rglob(pattern) if path.is_file())
    return tuple(sorted(auxiliary_paths))


def _current_bot_configs_for_conflicts(warnings: list[str]) -> list[Config]:
    try:
        return _read_config_file(strict=True)
    except Exception as error:
        warnings.append(f"当前本地 Bot 配置无法读取，冲突预览已跳过: {type(error).__name__}")
        return []


def _build_archive_extract_dir(path_func: PathFunc) -> Path:
    timestamp = datetime.now().strftime("%Y%m%d-%H%M%S-%f")
    extract_dir = path_func.tmp_path / "legacy-import" / "archive" / timestamp
    extract_dir.mkdir(parents=True, exist_ok=True)
    return extract_dir


def _safe_extract_zip(archive_path: Path, extract_dir: Path) -> None:
    with zipfile.ZipFile(archive_path, "r") as archive:
        for member in archive.infolist():
            member_path = extract_dir / member.filename
            resolved_member_path = member_path.resolve()
            if not str(resolved_member_path).startswith(str(extract_dir.resolve())):
                raise ValueError(f"ZIP 导入包包含非法路径: {member.filename}")
        archive.extractall(extract_dir)


def _scan_legacy_config_root(
    scan_root_path: Path,
    *,
    source_path: Path,
    source_kind: Literal["directory", "zip"],
    cleanup_path: Path | None = None,
    initial_warnings: list[str] | None = None,
) -> ImportScanResult:
    """扫描已准备好的导入根目录。"""

    if not scan_root_path.exists():
        raise FileNotFoundError(f"旧版配置目录不存在: {scan_root_path}")
    if not scan_root_path.is_dir():
        raise NotADirectoryError(f"拖入目标不是文件夹: {scan_root_path}")

    warnings = list(initial_warnings or [])
    app_config_path = _select_best_app_config(scan_root_path, warnings)
    bot_config_path, imported_bot_configs = _select_best_bot_config(scan_root_path, warnings)
    auxiliary_paths = _collect_auxiliary_paths(scan_root_path)

    current_configs = _current_bot_configs_for_conflicts(warnings)
    current_by_qqid = {int(config.bot.QQID): config for config in current_configs}
    conflicts = tuple(
        ImportConflictItem(
            qqid=int(config.bot.QQID),
            current_name=current_by_qqid[int(config.bot.QQID)].bot.name,
            imported_name=config.bot.name,
        )
        for config in imported_bot_configs
        if int(config.bot.QQID) in current_by_qqid
    )

    if app_config_path is None and bot_config_path is None:
        warnings.append("未识别到可导入的旧版 config.json 或 bot.json")

    return ImportScanResult(
        source_path=source_path,
        scan_root_path=scan_root_path,
        source_kind=source_kind,
        app_config_path=app_config_path,
        bot_config_path=bot_config_path,
        auxiliary_paths=auxiliary_paths,
        imported_bot_count=len(imported_bot_configs),
        conflicts=conflicts,
        warnings=tuple(warnings),
        cleanup_path=cleanup_path,
    )


def scan_legacy_import_source(path: Path) -> ImportScanResult:
    """扫描旧版配置导入源，支持目录和 ZIP 包。"""

    source_path = path.expanduser().resolve()
    if not source_path.exists():
        raise FileNotFoundError(f"导入源不存在: {source_path}")

    if source_path.is_dir():
        return _scan_legacy_config_root(
            source_path,
            source_path=source_path,
            source_kind="directory",
        )

    if source_path.suffix.lower() != ".zip":
        raise ValueError(f"当前仅支持导入文件夹或 ZIP 包: {source_path.name}")

    archive_size = source_path.stat().st_size
    if archive_size > _MAX_IMPORT_ARCHIVE_SIZE_BYTES:
        max_mb = _MAX_IMPORT_ARCHIVE_SIZE_BYTES // (1024 * 1024)
        actual_mb = archive_size / (1024 * 1024)
        raise ValueError(f"ZIP 导入包过大: {actual_mb:.1f} MB，当前限制为 {max_mb} MB")

    path_func = _get_path_func()
    extract_dir = _build_archive_extract_dir(path_func)
    _safe_extract_zip(source_path, extract_dir)
    return _scan_legacy_config_root(
        extract_dir,
        source_path=source_path,
        source_kind="zip",
        cleanup_path=extract_dir,
        initial_warnings=["当前正在从 ZIP 导入包中读取配置，导入完成后会自动清理临时解包目录"],
    )


def scan_legacy_config_folder(folder: Path) -> ImportScanResult:
    """扫描旧版配置目录。"""
    return scan_legacy_import_source(folder)


def _load_import_app_payload(path: Path) -> dict[str, object]:
    payload = _read_json_file(path)
    migrated_payload, config_version, _ = _migrate_config_payload(payload)
    _write_config_version(migrated_payload, config_version)
    return migrated_payload


def _load_import_bot_configs(path: Path) -> list[Config]:
    return _load_bot_configs(path)


def _merge_bot_configs(
    current_configs: list[Config],
    imported_configs: list[Config],
    mode: Literal["replace", "append"],
    overwrite_conflict_qqids: frozenset[int],
) -> tuple[list[Config], int, int, int]:
    if mode == "replace":
        current_ids = {int(config.bot.QQID) for config in current_configs}
        imported_ids = {int(config.bot.QQID) for config in imported_configs}
        replaced_count = len(current_ids & imported_ids)
        appended_count = len(imported_ids - current_ids)
        return imported_configs, replaced_count, appended_count, 0

    final_configs = current_configs.copy()
    index_by_qqid = {int(config.bot.QQID): index for index, config in enumerate(final_configs)}
    replaced_count = 0
    appended_count = 0
    skipped_count = 0

    for imported_config in imported_configs:
        qqid = int(imported_config.bot.QQID)
        if qqid not in index_by_qqid:
            final_configs.append(imported_config)
            index_by_qqid[qqid] = len(final_configs) - 1
            appended_count += 1
            continue

        if qqid in overwrite_conflict_qqids:
            final_configs[index_by_qqid[qqid]] = imported_config
            replaced_count += 1
        else:
            skipped_count += 1

    return final_configs, replaced_count, appended_count, skipped_count


def _create_backup_dir(path_func: PathFunc) -> Path:
    timestamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    backup_dir = path_func.tmp_path / "legacy-import" / timestamp
    backup_dir.mkdir(parents=True, exist_ok=True)
    return backup_dir


def _snapshot_paths(paths: list[Path], backup_dir: Path) -> None:
    files_dir = backup_dir / "files"
    files_dir.mkdir(parents=True, exist_ok=True)
    for path in paths:
        if not path.exists() or not path.is_file():
            continue
        shutil.copy2(path, files_dir / path.name)


def apply_legacy_config_import(plan: ImportExecutionPlan) -> ImportExecutionResult:
    """执行旧版配置导入。"""
    path_func = _get_path_func()
    payloads: dict[Path, object] = {}
    deletions: list[Path] = []
    warnings = list(plan.scan_result.warnings)
    backup_dir = _create_backup_dir(path_func)

    app_imported = False
    bot_imported = False
    imported_bot_count = 0
    replaced_bot_count = 0
    appended_bot_count = 0
    skipped_bot_count = 0

    backup_targets = [path_func.config_path, path_func.bot_config_path]

    if plan.import_app_config:
        if plan.scan_result.app_config_path is None:
            raise ValueError("执行计划要求导入应用配置，但扫描结果中不存在 config.json")
        payloads[path_func.config_path] = _load_import_app_payload(plan.scan_result.app_config_path)
        app_imported = True

    if plan.bot_import_mode != "skip":
        if plan.scan_result.bot_config_path is None:
            raise ValueError("执行计划要求导入 Bot 配置，但扫描结果中不存在 bot.json")

        current_configs = _read_config_file(strict=True)
        imported_configs = _load_import_bot_configs(plan.scan_result.bot_config_path)
        imported_bot_count = len(imported_configs)

        final_configs, replaced_bot_count, appended_bot_count, skipped_bot_count = _merge_bot_configs(
            current_configs=current_configs,
            imported_configs=imported_configs,
            mode=plan.bot_import_mode,
            overwrite_conflict_qqids=plan.overwrite_conflict_qqids,
        )

        payloads[path_func.bot_config_path] = serialize_bot_config_collection(final_configs)
        for config in final_configs:
            payloads[path_func.napcat_config_path / f"onebot11_{config.bot.QQID}.json"] = _model_to_payload(
                _build_onebot_config(config)
            )
            payloads[path_func.napcat_config_path / f"napcat_{config.bot.QQID}.json"] = _model_to_payload(
                _build_napcat_config(config)
            )

        final_ids = {int(config.bot.QQID) for config in final_configs}
        for config in current_configs:
            qqid = int(config.bot.QQID)
            if qqid in final_ids:
                continue
            deletions.append(path_func.napcat_config_path / f"onebot11_{qqid}.json")
            deletions.append(path_func.napcat_config_path / f"napcat_{qqid}.json")

        for payload_path in payloads:
            if payload_path.parent == path_func.napcat_config_path:
                backup_targets.append(payload_path)
        backup_targets.extend(deletions)
        bot_imported = True

    if not payloads:
        raise ValueError("当前导入计划未选择任何可执行的配置项")

    _snapshot_paths(backup_targets, backup_dir)
    _apply_json_transaction(payloads, deletions)

    logger.info(
        (
            "旧版配置导入完成: "
            f"app_imported={app_imported}, bot_imported={bot_imported}, "
            f"imported_bot_count={imported_bot_count}, replaced_bot_count={replaced_bot_count}, "
            f"appended_bot_count={appended_bot_count}, skipped_bot_count={skipped_bot_count}, "
            f"backup_dir={backup_dir}, app_target_version={_CURRENT_CONFIG_COMPAT_VERSION}, "
            f"bot_target_version={BOT_CONFIG_COMPAT_VERSION}"
        )
    )

    return ImportExecutionResult(
        app_imported=app_imported,
        bot_imported=bot_imported,
        imported_bot_count=imported_bot_count,
        replaced_bot_count=replaced_bot_count,
        appended_bot_count=appended_bot_count,
        skipped_bot_count=skipped_bot_count,
        backup_dir=backup_dir,
        warnings=tuple(warnings),
    )
