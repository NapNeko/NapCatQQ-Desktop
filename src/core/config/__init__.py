# -*- coding: utf-8 -*-

"""
这个模块包含的是 NapCatQQ Desktop 的配置项

每个配置项都对应一个类变量, 变量名即为配置项的键名
配置项的类型可以是以下几种:
- ConfigItem: 基础配置项, 支持 str, int, float, bool 等基本类型
- OptionsConfigItem: 选项配置项, 支持枚举类型和指定选项列表
- RangeConfigItem: 范围配置项, 支持数值类型, 并且有最小值和最大值限制
- ColorConfigItem: 颜色配置项, 支持颜色字符串, 如 "#RRGGBB" 或 "#AARRGGBB"
- 其他类型可以参考 qfluentwidgets.common 模块

每个配置项都可以指定以下参数:
- group: 配置项所属的组名, 用于分类显示
- name: 配置项的键名, 用于存储和读取配置文件
- default: 配置项的默认值, 如果配置文件中没有该项, 则使用默认值
- validator: 配置项的验证器, 用于验证配置项的值是否合法
- serializer: 配置项的序列化器, 用于将配置项的值转换为字符串存储到配置文件中, 或从字符串读取并转换为配置项的值
- restart: 是否需要重启应用程序才能生效, 如果为 True, 则在修改该配置项后会发出 app_restart_signal 信号
- 其他参数可以参考 qfluentwidgets.common 模块

注意:
- 配置文件默认存储在用户的配置目录下, 可以通过 PathFunc().config_path 获取
- 修改配置项后, 需要调用 cfg.set(item, value) 方法来设置新的值, 这样才能触发验证和序列化
- 如果配置项的值不合法, 则会抛出 ValueError 异常
- 如果配置文件不存在或格式错误, 则会使用默认配置
- 如果程序更新后, 有新增的配置项, 则会自动添加默认值(此方法在 Config.load 方法中实现, 而非 QConfig.load)
- 具体使用方法可以参考 qfluentwidgets.common.QConfig 类的文档
- 与 config_model.py 区别在于, config_model.py 主要用于定义与机器人相关的配置模型, 而此模块主要用于定义应用程序的整体配置项
"""

# 标准库导入
import contextlib
import inspect
import json
import os
import platform
import time
import uuid
import warnings
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Self

# 第三方库导入
from creart import it
from qfluentwidgets.common import (
    BoolValidator,
    ColorConfigItem,
    ConfigItem,
    ConfigSerializer,
    EnumSerializer,
    OptionsConfigItem,
    OptionsValidator,
    QConfig,
    RangeConfigItem,
    RangeValidator,
    Theme,
    qconfig,
)
from qfluentwidgets.common.exception_handler import exceptionHandler
from PySide6.QtCore import QLocale, Signal

# 项目内模块导入
from src.core.config.config_enum import CloseActionEnum, Language
from src.core.logging import LogSource, logger
from src.core.remote.models import LinuxCorePaths, SSHCredentials
from src.core.runtime.paths import PathFunc

__version__ = "v2.0.19"
_CONFIG_MIGRATION_BACKUP_SUFFIX = ".bak"
_CONFIG_MIGRATION_TMP_MARKER = "tmp"
_LEGACY_CONFIG_VERSION = "v1.7.28"
_CURRENT_CONFIG_COMPAT_VERSION = "v2.0"
_LEGACY_BACKGROUND_KEYS = (
    "BgHomePage",
    "BgHomePageOpacity",
    "BgHomePageLight",
    "BgHomePageDark",
    "BgAddPage",
    "BgAddPageOpacity",
    "BgAddPageLight",
    "BgAddPageDark",
    "BgListPage",
    "BgListPageOpacity",
    "BgListPageLight",
    "BgListPageDark",
    "BgUnitPage",
    "BgUnitPageOpacity",
    "BgUnitPageLight",
    "BgUnitPageDark",
    "BgSettingPage",
    "BgSettingPageOpacity",
    "BgSettingPageLight",
    "BgSettingPageDark",
)
_LEGACY_TITLE_TAB_BAR_KEYS = (
    "TitleTabBar",
    "TitleTabBarIsMovable",
    "TitleTabBarIsScrollable",
    "TitleTabBarIsShadow",
    "TitleTabBarCloseButton",
    "TitleTabBarMinWidth",
    "TitleTabBarMaxWidth",
)


@dataclass(frozen=True)
class ConfigMigrationStep:
    """单步配置迁移规则。

    扩展约定:
    1. 这里的版本是“配置兼容版本”，不是应用发布版本。
    2. 只有配置结构发生不兼容调整时，才提升 `_CURRENT_CONFIG_COMPAT_VERSION`。
    3. 如果程序升级了，但配置结构完全兼容，就不要改配置版本号。
    4. 只定义 `from_version -> to_version` 的单步迁移，不要跨多版本跳跃。
    5. 新规则只做“补齐/搬运/规范化”，不要覆盖用户已经存在的新结构值。
    6. 新增结构变更时，先追加一个迁移函数，再把它注册到 `_CONFIG_MIGRATION_STEPS`。

    例子:
    - 程序从 v2.3 升到 v3.0，但配置结构没变:
      `_CURRENT_CONFIG_COMPAT_VERSION` 仍然可以保持为 `v2.0`
    - 只有配置结构真的从 v2 系切到 v3 系时，才新增 `v2.0 -> v3.0` 迁移
    """

    from_version: str
    to_version: str
    description: str
    apply: Callable[[dict[str, object]], list[str]]


def _deep_copy_json(data: Any) -> Any:
    """复制 JSON 兼容结构，避免迁移时原地修改读取结果。"""
    return json.loads(json.dumps(data, ensure_ascii=False))


def _next_transaction_path(path: Path, marker: str) -> Path:
    """生成配置迁移阶段使用的临时路径。"""
    return path.with_name(f"{path.name}.{marker}.{uuid.uuid4().hex}")


def _ensure_object(value: object) -> dict[str, object]:
    """确保配置节点为对象，异常结构时安全重建为空对象。"""
    if isinstance(value, dict):
        return value
    return {}


def _set_nested_value(payload: dict[str, object], path: tuple[str, ...], value: object) -> bool:
    """仅在目标键缺失时设置值，返回是否实际写入。"""
    current = payload
    for segment in path[:-1]:
        child = current.get(segment)
        if not isinstance(child, dict):
            child = {}
            current[segment] = child
        current = child

    leaf = path[-1]
    if leaf in current:
        return False

    current[leaf] = value
    return True


def _pop_nested_value(payload: dict[str, object], path: tuple[str, ...]) -> object | None:
    """弹出嵌套键。"""
    current: dict[str, object] = payload
    for segment in path[:-1]:
        child = current.get(segment)
        if not isinstance(child, dict):
            return None
        current = child
    return current.pop(path[-1], None)


def _remove_nested_key(payload: dict[str, object], path: tuple[str, ...]) -> bool:
    """删除嵌套键并清理空对象。"""
    parents: list[tuple[dict[str, object], str]] = []
    current: dict[str, object] = payload
    for segment in path[:-1]:
        child = current.get(segment)
        if not isinstance(child, dict):
            return False
        parents.append((current, segment))
        current = child

    if path[-1] not in current:
        return False

    del current[path[-1]]
    while parents and not current:
        parent, segment = parents.pop()
        del parent[segment]
        current = parent
    return True


def _move_nested_value(payload: dict[str, object], source: tuple[str, ...], target: tuple[str, ...]) -> bool:
    """在目标键缺失时搬运旧值。"""
    value = _pop_nested_value(payload, source)
    if value is None:
        return False
    if not _set_nested_value(payload, target, value):
        _set_nested_value(payload, source, value)
        return False
    return True


def _cleanup_empty_sections(payload: dict[str, object]) -> None:
    """清理迁移后残留的空分组。"""
    empty_keys = [key for key, value in payload.items() if isinstance(value, dict) and not value]
    for key in empty_keys:
        del payload[key]


def _migrate_app_v154_to_v160(payload: dict[str, object]) -> list[str]:
    """v1.5.4 -> v1.6.0: 清理旧背景项并规范 CloseBtnAction 分组。"""
    rules_applied: list[str] = []
    if _move_nested_value(payload, ("Personalized", "CloseBtnAction"), ("General", "CloseBtnAction")):
        rules_applied.append("Personalized.CloseBtnAction -> General.CloseBtnAction")

    for key in _LEGACY_BACKGROUND_KEYS:
        if _remove_nested_key(payload, ("Personalize", key)):
            rules_applied.append(f"Personalize.{key} removed")

    if _remove_nested_key(payload, ("HideTips", "HideUsingGoBtnTips")):
        rules_applied.append("HideTips.HideUsingGoBtnTips removed")

    _cleanup_empty_sections(payload)
    return rules_applied


def _migrate_app_v160_to_v170(payload: dict[str, object]) -> list[str]:
    """v1.6.0 -> v1.7.0: 处理 MainWindow 键名调整。"""
    rules_applied: list[str] = []
    if _move_nested_value(payload, ("Info", "main_window"), ("Info", "MainWindow")):
        rules_applied.append("Info.main_window -> Info.MainWindow")
    return rules_applied


def _migrate_app_v170_to_v1728(payload: dict[str, object]) -> list[str]:
    """v1.7.0 -> v1.7.28: 补齐 EULA 并清理 TitleTabBar 历史键。"""
    rules_applied: list[str] = []
    if _set_nested_value(payload, ("Info", "EulaAccepted"), False):
        rules_applied.append("Info.EulaAccepted default")

    for key in _LEGACY_TITLE_TAB_BAR_KEYS:
        if _remove_nested_key(payload, ("Personalize", key)):
            rules_applied.append(f"Personalize.{key} removed")

    _cleanup_empty_sections(payload)
    return rules_applied


def _migrate_app_v1728_to_v20(payload: dict[str, object]) -> list[str]:
    """v1.7.28 -> v2.0: 同步 QFluentWidgets 和 Home 字段。"""
    rules_applied: list[str] = []
    personalize = _ensure_object(payload.get("Personalize"))
    if "Personalize" in payload:
        payload["Personalize"] = personalize

    theme_mode = personalize.get("ThemeMode")
    if theme_mode is not None and _set_nested_value(payload, ("QFluentWidgets", "ThemeMode"), theme_mode):
        rules_applied.append("Personalize.ThemeMode -> QFluentWidgets.ThemeMode")

    theme_color = personalize.get("ThemeColor")
    if theme_color is not None and _set_nested_value(payload, ("QFluentWidgets", "ThemeColor"), theme_color):
        rules_applied.append("Personalize.ThemeColor -> QFluentWidgets.ThemeColor")

    if _set_nested_value(payload, ("Home", "IgnoredNoticeKeys"), "[]"):
        rules_applied.append("Home.IgnoredNoticeKeys default")
    if _set_nested_value(payload, ("Home", "SnoozedNoticeItems"), "{}"):
        rules_applied.append("Home.SnoozedNoticeItems default")

    return rules_applied


_CONFIG_MIGRATION_STEPS: tuple[ConfigMigrationStep, ...] = (
    ConfigMigrationStep(
        from_version="v1.5.4",
        to_version="v1.6.0",
        description="清理旧背景项并规范 CloseBtnAction 分组",
        apply=_migrate_app_v154_to_v160,
    ),
    ConfigMigrationStep(
        from_version="v1.6.0",
        to_version="v1.7.0",
        description="处理 MainWindow 键名调整",
        apply=_migrate_app_v160_to_v170,
    ),
    ConfigMigrationStep(
        from_version="v1.7.0",
        to_version="v1.7.28",
        description="补齐 EulaAccepted 并清理 TitleTabBar 历史键",
        apply=_migrate_app_v170_to_v1728,
    ),
    ConfigMigrationStep(
        from_version="v1.7.28",
        to_version="v2.0",
        description="同步 QFluentWidgets 和 Home 字段",
        apply=_migrate_app_v1728_to_v20,
    ),
)


def _read_config_version(payload: dict[str, object]) -> str:
    """读取配置兼容版本。

    规则:
    - 若 `Info.ConfigVersion` 存在且非空，优先使用。
    - 若缺失该字段，则统一视为 `v1.7.28` 及以下的旧配置。
    - `ConfigSchemaVersion` 是历史实验字段，如果用户本地残留了它，
      当前阶段统一按“已迁移到当前兼容版本”处理，后续可在正式迁移规则中清理。
    """
    info = _ensure_object(payload.get("Info"))
    config_version = info.get("ConfigVersion")
    if isinstance(config_version, str) and config_version.strip():
        return config_version.strip()

    if "ConfigSchemaVersion" in info:
        return _CURRENT_CONFIG_COMPAT_VERSION

    personalize = _ensure_object(payload.get("Personalize"))
    if any(key in personalize for key in _LEGACY_BACKGROUND_KEYS):
        return "v1.5.4"
    if "HideUsingGoBtnTips" in _ensure_object(payload.get("HideTips")):
        return "v1.5.4"

    if "main_window" in info:
        return "v1.6.0"

    if any(key in personalize for key in _LEGACY_TITLE_TAB_BAR_KEYS) or "EulaAccepted" not in info:
        return "v1.7.0"

    return _LEGACY_CONFIG_VERSION


def _has_explicit_config_version(payload: dict[str, object]) -> bool:
    """判断配置文件是否已显式写入 ConfigVersion。"""
    info = _ensure_object(payload.get("Info"))
    value = info.get("ConfigVersion")
    return isinstance(value, str) and bool(value.strip())


def _write_config_version(payload: dict[str, object], version: str) -> bool:
    """写入配置兼容版本，返回是否发生变化。"""
    info = _ensure_object(payload.get("Info"))
    payload["Info"] = info
    if info.get("ConfigVersion") == version:
        return False
    info["ConfigVersion"] = version
    return True


def _migrate_config_payload(payload: object) -> tuple[dict[str, object], str, list[str]]:
    """将旧版应用配置按兼容版本逐步迁移到当前结构。

    注意:
    - 这里的“版本”是配置兼容版本，不是应用发布版本。
    - 若程序版本提升但配置结构不变，可继续保持同一个配置兼容版本。
    - 迁移按 `vN -> vN+1` 顺序执行，避免直接从很老的结构跳到最新结构。
    - 每一步都应保持幂等，重复执行不会覆盖用户的最新配置。
    - 当前只保留迁移扩展口；具体规则等后续配置结构再次调整时再补。
    """
    if not isinstance(payload, dict):
        return {}, _LEGACY_CONFIG_VERSION, []

    migrated = _deep_copy_json(payload)
    current_version = _read_config_version(migrated)
    rules_applied: list[str] = []

    visited_versions: set[str] = set()
    while current_version != _CURRENT_CONFIG_COMPAT_VERSION and current_version not in visited_versions:
        visited_versions.add(current_version)
        step = next((item for item in _CONFIG_MIGRATION_STEPS if item.from_version == current_version), None)
        if step is None:
            break

        step_rules = step.apply(migrated)
        if step_rules:
            rules_applied.extend(
                f"{step.from_version}->{step.to_version}: {rule}" for rule in step_rules
            )
        else:
            rules_applied.append(f"{step.from_version}->{step.to_version}: no-op")
        current_version = step.to_version

    return migrated, current_version, rules_applied


def _persist_migrated_config(path: Path, payload: dict[str, object]) -> Path | None:
    """以原子替换方式写回迁移后的配置，并保留一份备份。"""
    path.parent.mkdir(parents=True, exist_ok=True)
    temp_path = _next_transaction_path(path, _CONFIG_MIGRATION_TMP_MARKER)
    backup_path = path.with_name(f"{path.name}{_CONFIG_MIGRATION_BACKUP_SUFFIX}")
    backup_created = False

    try:
        with open(temp_path, "w", encoding="utf-8") as file:
            json.dump(payload, file, ensure_ascii=False, indent=4)

        if path.exists() and not backup_path.exists():
            os.replace(path, backup_path)
            backup_created = True

        os.replace(temp_path, path)
        return backup_path if backup_created else None
    except Exception:
        with contextlib.suppress(FileNotFoundError):
            temp_path.unlink()
        if backup_created and backup_path.exists() and not path.exists():
            with contextlib.suppress(OSError):
                os.replace(backup_path, path)
        raise


class LanguageSerializer(ConfigSerializer):
    """语言序列化"""

    def serialize(self, value):
        return value.value.name() if value != Language.AUTO else "Auto"

    def deserialize(self, value: str):
        return Language(QLocale(value)) if value != "Auto" else Language.AUTO


class Config(QConfig):
    """程序配置"""

    # 信号
    app_restart_signal = Signal()

    # 信息项
    napcat_desktop_version = ConfigItem(group="Info", name="NCDVersion", default="")
    config_version = ConfigItem(group="Info", name="ConfigVersion", default=_CURRENT_CONFIG_COMPAT_VERSION)
    start_time = ConfigItem(group="Info", name="StartTime", default="")
    system_type = ConfigItem(group="Info", name="SystemType", default="")
    platform_type = ConfigItem(group="Info", name="PlatformType", default="")
    main_window = ConfigItem(group="Info", name="MainWindow", default=False, validator=BoolValidator())
    elua_accepted = ConfigItem(group="Info", name="EulaAccepted", default=False, validator=BoolValidator())

    # 个性化项目
    language = OptionsConfigItem(
        group="Personalize",
        name="Language",
        default=Language.AUTO,
        validator=OptionsValidator(Language),
        serializer=LanguageSerializer(),
        restart=True,
    )
    theme_mode = OptionsConfigItem(
        group="Personalize",
        name="ThemeMode",
        default=Theme.AUTO,
        validator=OptionsValidator(Theme),
        serializer=EnumSerializer(Theme),
    )
    theme_color = ColorConfigItem(
        group="Personalize",
        name="ThemeColor",
        default="#009faa",
    )
    dpi_scale = OptionsConfigItem(
        group="Personalized",
        name="DpiScale",
        default="Auto",
        validator=OptionsValidator([1, 1.25, 1.5, 1.75, 2, "Auto"]),
        restart=True,
    )
    close_button_action = OptionsConfigItem(
        group="General",
        name="CloseBtnAction",
        default=CloseActionEnum.CLOSE,
        validator=OptionsValidator(list(CloseActionEnum)),
        serializer=EnumSerializer(CloseActionEnum),
    )
    window_opacity = RangeConfigItem(
        group="Personalize",
        name="WindowOpacity",
        default=100,
        validator=RangeValidator(10, 100),
    )

    # 事件项
    bot_offline_email_notice = ConfigItem(
        group="Event", name="BotOfflineEmailNotice", default=False, validator=BoolValidator()
    )
    bot_offline_web_hook_notice = ConfigItem(
        group="Event", name="BotOfflineWebHookNotice", default=False, validator=BoolValidator()
    )

    # 邮件项
    email_receiver = ConfigItem(group="Email", name="EmailReceiver", default="")
    email_sender = ConfigItem(group="Email", name="EmailSender", default="")
    email_token = ConfigItem(group="Email", name="EmailToken", default="")
    email_stmp_server = ConfigItem(group="Email", name="EmailStmpServer", default="")
    email_stmp_port = RangeConfigItem(
        group="Email",
        name="EmailStmpPort",
        default=465,
        validator=RangeValidator(1, 65535),
    )
    email_encryption = OptionsConfigItem(
        group="Email",
        name="EmailEncryption",
        default="SSL",
        validator=OptionsValidator(["SSL", "TLS", "无加密"]),
    )

    # WebHook 项
    web_hook_url = ConfigItem(group="WebHook", name="WebHookUrl", default="")
    web_hook_secret = ConfigItem(group="WebHook", name="WebHookSecret", default="")
    web_hook_json = ConfigItem(group="WebHook", name="WebHookJson", default="")
    web_hook_method = OptionsConfigItem(
        group="WebHook",
        name="WebHookMethod",
        default="POST",
        validator=OptionsValidator(["POST", "GET"]),
    )

    # 首页通知
    home_notice_ignored_keys = ConfigItem(group="Home", name="IgnoredNoticeKeys", default="[]")
    home_notice_snoozed_items = ConfigItem(group="Home", name="SnoozedNoticeItems", default="{}")

    # 远程模式项
    remote_enabled = ConfigItem(group="Remote", name="Enabled", default=False, validator=BoolValidator())
    remote_host = ConfigItem(group="Remote", name="Host", default="")
    remote_port = RangeConfigItem(group="Remote", name="Port", default=22, validator=RangeValidator(1, 65535))
    remote_username = ConfigItem(group="Remote", name="Username", default="")
    remote_auth_method = OptionsConfigItem(
        group="Remote",
        name="AuthMethod",
        default="key",
        validator=OptionsValidator(["key", "password"]),
    )
    remote_private_key_path = ConfigItem(group="Remote", name="PrivateKeyPath", default="")
    remote_allow_agent = ConfigItem(group="Remote", name="AllowAgent", default=False, validator=BoolValidator())
    remote_look_for_keys = ConfigItem(group="Remote", name="LookForKeys", default=False, validator=BoolValidator())
    remote_host_key_policy = OptionsConfigItem(
        group="Remote",
        name="HostKeyPolicy",
        default="reject",
        validator=OptionsValidator(["reject", "warning", "auto_add"]),
    )
    remote_connect_timeout = RangeConfigItem(
        group="Remote",
        name="ConnectTimeout",
        default=10,
        validator=RangeValidator(1, 120),
    )
    remote_command_timeout = RangeConfigItem(
        group="Remote",
        name="CommandTimeout",
        default=20,
        validator=RangeValidator(1, 600),
    )
    remote_workspace_dir = ConfigItem(group="Remote", name="WorkspaceDir", default="$HOME/NapCatCore")

    def __init__(self):
        super().__init__()

        # 修改 Config 初始化
        self.fontFamilies.restart = True

        # 将 QFluentWidgets 的重启信号转发给 UI 层，由 UI 决定如何提示用户
        self.appRestartSig.connect(self.app_restart_signal.emit)

    @exceptionHandler()
    def load(self, file: str | Path | None = None, config: Self | None = None) -> None:
        """重写加载配置

        能够在程序更新后,如果有新增的配置项,能自动添加默认值

        此处的 file 和 config 参数,与 QConfig.load 的参数意义相同
        但优先级为 config > file > self._cfg

        Args:
            file (str | Path | None): 配置文件路径，可为 None。
            config (Self | None): 需要加载的配置类，可为 None。
        """

        # 处理 config 和 file 参数
        if isinstance(config, QConfig):
            self._cfg = config
            self._cfg.themeChanged.connect(self.themeChanged)

        if isinstance(file, (str, Path)):
            self._cfg.file = Path(file)

        logger.trace(f"开始加载配置文件: path={self._cfg.file}", log_source=LogSource.CORE)

        # 加载配置文件
        try:
            with open(self._cfg.file, "r", encoding="utf-8") as f:
                cfg = json.load(f)
        except (FileNotFoundError, json.JSONDecodeError) as e:
            logger.error(f"配置文件加载失败: {e.__class__.__name__}: {str(e)}, 使用默认配置")
            cfg = {}
        else:
            has_explicit_config_version = _has_explicit_config_version(cfg)
            cfg, config_version, migration_rules = _migrate_config_payload(cfg)
            version_written = _write_config_version(cfg, config_version)
            logger.trace(
                f"检测到配置兼容版本: path={self._cfg.file}, config_version={config_version}",
                log_source=LogSource.CORE,
            )
            if migration_rules or (version_written and not has_explicit_config_version):
                try:
                    backup_path = _persist_migrated_config(Path(self._cfg.file), cfg)
                except Exception as migration_error:
                    logger.error(
                        (
                            "应用配置迁移写回失败，将继续使用内存中的迁移结果: "
                            f"path={self._cfg.file}, target_config_version={_CURRENT_CONFIG_COMPAT_VERSION}, "
                            f"rules={migration_rules}, error={type(migration_error).__name__}: {migration_error}"
                        ),
                        log_source=LogSource.CORE,
                    )
                else:
                    logger.info(
                        (
                            "应用配置迁移/版本标记完成: "
                            f"path={self._cfg.file}, target_config_version={_CURRENT_CONFIG_COMPAT_VERSION}, "
                            f"rules={migration_rules}, "
                            f"backup={backup_path if backup_path else 'existing-or-skipped'}"
                        ),
                        log_source=LogSource.CORE,
                    )

        # 获取配置项
        items = {
            item.key: item for _, item in inspect.getmembers(self._cfg.__class__, lambda x: isinstance(x, ConfigItem))
        }

        # 展平配置字典
        def flatten(d, parent_key: str = ""):
            """展平嵌套字典, 键用点连接"""
            for k, v in d.items():
                key = f"{parent_key}.{k}" if parent_key else k
                if isinstance(v, dict):
                    yield from flatten(v, key)
                else:
                    yield key, v

        flat_cfg = dict(flatten(cfg))

        for item in items.values():
            item.value = item.defaultValue

        # 更新配置项
        for k, v in flat_cfg.items():
            if k in items:
                items[k].deserializeFrom(v)

        self._sync_theme_config_items(flat_cfg)

        # 应用主题
        self.theme = self.get(self.themeMode)
        logger.trace(
            (
                "配置加载完成: "
                f"path={self._cfg.file}, flat_keys={len(flat_cfg)}, "
                f"matched_items={sum(1 for key in flat_cfg if key in items)}, theme={self.theme}"
            ),
            log_source=LogSource.CORE,
        )

    def _sync_theme_config_items(self, flat_cfg: dict[str, object]) -> None:
        """兼容旧版 Personalize 主题字段，并保持与 QFluentWidgets 字段一致。"""
        has_legacy_theme_mode = self.theme_mode.key in flat_cfg
        has_fluent_theme_mode = self.themeMode.key in flat_cfg
        has_legacy_theme_color = self.theme_color.key in flat_cfg
        has_fluent_theme_color = self.themeColor.key in flat_cfg

        if has_legacy_theme_mode and not has_fluent_theme_mode:
            self.themeMode.value = self.get(self.theme_mode)
        else:
            self.theme_mode.value = self.get(self.themeMode)

    def build_ssh_credentials(self) -> SSHCredentials:
        """从当前配置构建 SSH 凭据对象。

        安全策略：
        - 不从配置文件读取或持久化密码
        - 默认拒绝未知主机指纹
        - 默认关闭 agent 与自动扫描本地密钥
        """
        return SSHCredentials(
            host=str(self.get(self.remote_host) or ""),
            port=int(self.get(self.remote_port) or 22),
            username=str(self.get(self.remote_username) or ""),
            auth_method=str(self.get(self.remote_auth_method) or "key"),
            private_key_path=str(self.get(self.remote_private_key_path) or "") or None,
            connect_timeout=float(self.get(self.remote_connect_timeout) or 10),
            command_timeout=float(self.get(self.remote_command_timeout) or 20),
            host_key_policy=str(self.get(self.remote_host_key_policy) or "reject"),
            allow_agent=bool(self.get(self.remote_allow_agent)),
            look_for_keys=bool(self.get(self.remote_look_for_keys)),
        )

    def build_linux_core_paths(self) -> LinuxCorePaths:
        """从当前配置构建 Linux Core 路径布局。"""
        workspace_dir = str(self.get(self.remote_workspace_dir) or "$HOME/NapCatCore")
        workspace_dir = workspace_dir.rstrip("/")
        return LinuxCorePaths(
            workspace_dir=workspace_dir,
            runtime_dir=f"{workspace_dir}/runtime",
            config_dir=f"{workspace_dir}/runtime/config",
            log_dir=f"{workspace_dir}/runtime/log",
            tmp_dir=f"{workspace_dir}/runtime/tmp",
            package_dir=f"{workspace_dir}/packages",
        )

        if has_legacy_theme_color and not has_fluent_theme_color:
            self.themeColor.value = self.get(self.theme_color)
        else:
            self.theme_color.value = self.get(self.themeColor)


def bind_qfluent_qconfig(config: Config) -> None:
    """将 QFluentWidgets 全局 qconfig 绑定到项目配置，避免写到仓库根目录。"""
    qconfig._cfg = config
    qconfig.file = Path(config.file)
    qconfig.theme = config.get(config.themeMode)

    for signal_name, relay_name in (
        ("themeChanged", "_napcat_theme_changed_relay"),
        ("themeColorChanged", "_napcat_theme_color_changed_relay"),
        ("appRestartSig", "_napcat_app_restart_relay"),
    ):
        old_relay = getattr(qconfig, relay_name, None)
        if old_relay is not None:
            with warnings.catch_warnings():
                warnings.simplefilter("ignore", RuntimeWarning)
                with contextlib.suppress(RuntimeError, TypeError):
                    getattr(config, signal_name).disconnect(old_relay)

    def relay_theme_changed(theme: Theme) -> None:
        qconfig.themeChanged.emit(theme)

    def relay_theme_color_changed(color: object) -> None:
        qconfig.themeColorChanged.emit(color)

    def relay_app_restart() -> None:
        qconfig.appRestartSig.emit()

    setattr(qconfig, "_napcat_theme_changed_relay", relay_theme_changed)
    setattr(qconfig, "_napcat_theme_color_changed_relay", relay_theme_color_changed)
    setattr(qconfig, "_napcat_app_restart_relay", relay_app_restart)

    config.themeChanged.connect(relay_theme_changed)
    config.themeColorChanged.connect(relay_theme_color_changed)
    config.appRestartSig.connect(relay_app_restart)


cfg = Config()
cfg.load(it(PathFunc).config_path)
cfg.set(cfg.start_time, time.time(), True)
cfg.set(cfg.napcat_desktop_version, __version__, True)
cfg.set(cfg.system_type, platform.system(), True)
cfg.set(cfg.platform_type, platform.machine(), True)
bind_qfluent_qconfig(cfg)

