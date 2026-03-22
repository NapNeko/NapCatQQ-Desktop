# -*- coding: utf-8 -*-
# 标准库导入
import random
import re
import string
from typing import Any, Literal

# 第三方库导入
from pydantic import BaseModel, Field, HttpUrl, WebsocketUrl, field_validator, model_validator

from .config_enum import TimeUnitEnum

"""
这个模块包含的是机器人配置模型

每个配置模型都继承自 pydantic 的 BaseModel, 用于数据验证和序列化
每个配置模型都可以包含多个字段, 每个字段都对应一个配置项
每个字段都可以指定类型, 默认值, 验证器等

注意:
- 配置模型主要用于定义与机器人相关的配置项, 如机器人名称, QQ号, 连接方式等
- 配置模型可以嵌套, 如 ConnectConfig 包含多个网络连接配置模型
- 配置模型可以与 JSON 等格式互相转换, 方便存储和读取配置文件
- 具体使用方法可以参考 pydantic 的文档
- 此文件可以不遵守 naming_convention 中的命名规范, 因为 NapCatQQ 配置项采用驼峰体命名, 以保持与 OneBot 标准的一致
- 如果使用 snake_case 命名, 则需要在读取配置文件时进行转换, 增加不必要的复杂度
"""

BOT_CONFIG_LEGACY_VERSION = "v1.7.28"
BOT_CONFIG_COMPAT_VERSION = "v2.0"
DEFAULT_AUTO_RESTART_SCHEDULE_PAYLOAD = {
    "enable": False,
    "time_unit": TimeUnitEnum.HOUR.value,
    "duration": 6,
}
_LEGACY_AUTO_RESTART_INTERVAL_PATTERN = re.compile(r"^\s*(\d+)\s*(m|h|d|mon|year)\s*$", re.IGNORECASE)
_LOG_LEVEL_CHOICES = {"debug", "info", "error"}


def _clone_payload(data: Any) -> Any:
    """复制任意 JSON 兼容结构。"""
    import json

    return json.loads(json.dumps(data, ensure_ascii=False))


def _ensure_dict(value: object) -> dict[str, object]:
    """确保配置节点为对象，异常结构时安全重建为空对象。"""
    if isinstance(value, dict):
        return value
    return {}


def _ensure_list(value: object) -> list[object]:
    """确保配置节点为列表，异常结构时安全重建为空列表。"""
    if isinstance(value, list):
        return value
    return []


def _normalize_bool(value: object, default: bool = False) -> bool:
    """将旧配置中的布尔值规范化。"""
    if isinstance(value, bool):
        return value
    if isinstance(value, (int, float)):
        return bool(value)
    if isinstance(value, str):
        normalized = value.strip().lower()
        if normalized in {"1", "true", "yes", "on"}:
            return True
        if normalized in {"0", "false", "no", "off", ""}:
            return False
    return default


def _normalize_port(value: object, default: int) -> int:
    """规范化旧版端口配置。"""
    try:
        port = int(value)
    except (TypeError, ValueError):
        return default
    return port if port > 0 else default


def _normalize_log_level(*values: object, default: str) -> str:
    """从多个候选值中提取合法日志级别。"""
    for value in values:
        if isinstance(value, str):
            normalized = value.strip().lower()
            if normalized in _LOG_LEVEL_CHOICES:
                return normalized
    return default


def _extract_legacy_network_defaults(bot_payload: dict[str, object], advanced_payload: dict[str, object]) -> dict[str, object]:
    """提取旧版网络公共字段。"""
    return {
        "message_post_format": str(bot_payload.pop("messagePostFormat", "array") or "array"),
        "token": str(bot_payload.pop("token", "") or ""),
        "report_self_message": _normalize_bool(bot_payload.pop("reportSelfMessage", False), False),
        "heart_interval": _coerce_interval_default(bot_payload.pop("heartInterval", 30000), 30000),
        "debug": _normalize_bool(advanced_payload.pop("debug", False), False),
    }


def _has_meaningful_value(*values: object) -> bool:
    """判断旧结构中是否存在值得迁移的有效值。"""
    for value in values:
        if isinstance(value, bool) and value:
            return True
        if isinstance(value, (int, float)) and value != 0:
            return True
        if isinstance(value, str) and value.strip():
            return True
        if isinstance(value, list) and len(value) > 0:
            return True
        if isinstance(value, dict) and len(value) > 0:
            return True
    return False


def _build_legacy_http_server(
    http_payload: dict[str, object], network_defaults: dict[str, object]
) -> dict[str, object] | None:
    """将 v1.4/v1.5 的 http 节点迁移为当前 http server 结构。"""
    if not _has_meaningful_value(http_payload.get("enable"), http_payload.get("host"), http_payload.get("port")):
        return None

    return {
        "enable": _normalize_bool(http_payload.get("enable", False), False),
        "name": "legacy-http-server",
        "messagePostFormat": network_defaults["message_post_format"],
        "token": network_defaults["token"],
        "debug": network_defaults["debug"],
        "host": str(http_payload.get("host", "") or ""),
        "port": _normalize_port(http_payload.get("port", 3000), 3000),
        "enableCors": False,
        "enableWebsocket": False,
    }


def _build_legacy_http_clients(
    http_payload: dict[str, object], network_defaults: dict[str, object]
) -> list[dict[str, object]]:
    """将旧版 postUrls 迁移为 http client 列表。"""
    clients: list[dict[str, object]] = []
    for index, raw_url in enumerate(_ensure_list(http_payload.get("postUrls"))):
        if not isinstance(raw_url, str) or not raw_url.strip():
            continue
        clients.append(
            {
                "enable": _normalize_bool(http_payload.get("enablePost", True), True),
                "name": f"legacy-http-client-{index + 1}",
                "messagePostFormat": network_defaults["message_post_format"],
                "token": network_defaults["token"],
                "debug": network_defaults["debug"],
                "url": raw_url,
                "reportSelfMessage": network_defaults["report_self_message"],
            }
        )
    return clients


def _build_legacy_websocket_server(
    ws_payload: dict[str, object], network_defaults: dict[str, object]
) -> dict[str, object] | None:
    """将旧版 ws 节点迁移为当前 websocket server 结构。"""
    if not _has_meaningful_value(ws_payload.get("enable"), ws_payload.get("host"), ws_payload.get("port")):
        return None

    return {
        "enable": _normalize_bool(ws_payload.get("enable", False), False),
        "name": "legacy-websocket-server",
        "messagePostFormat": network_defaults["message_post_format"],
        "token": network_defaults["token"],
        "debug": network_defaults["debug"],
        "host": str(ws_payload.get("host", "") or ""),
        "port": _normalize_port(ws_payload.get("port", 3001), 3001),
        "reportSelfMessage": network_defaults["report_self_message"],
        "enableForcePushEvent": False,
        "heartInterval": network_defaults["heart_interval"],
    }


def _build_legacy_websocket_clients(
    reverse_ws_payload: dict[str, object], network_defaults: dict[str, object]
) -> list[dict[str, object]]:
    """将旧版 reverseWs 节点迁移为当前 websocket client 列表。"""
    clients: list[dict[str, object]] = []
    enabled = _normalize_bool(reverse_ws_payload.get("enable", False), False)
    for index, raw_url in enumerate(_ensure_list(reverse_ws_payload.get("urls"))):
        if not isinstance(raw_url, str) or not raw_url.strip():
            continue
        clients.append(
            {
                "enable": enabled,
                "name": f"legacy-websocket-client-{index + 1}",
                "messagePostFormat": network_defaults["message_post_format"],
                "token": network_defaults["token"],
                "debug": network_defaults["debug"],
                "url": raw_url,
                "reportSelfMessage": network_defaults["report_self_message"],
                "heartInterval": network_defaults["heart_interval"],
                "reconnectInterval": 30000,
            }
        )
    return clients


def _migrate_legacy_connect_shape(
    connect_payload: dict[str, object], bot_payload: dict[str, object], advanced_payload: dict[str, object]
) -> tuple[dict[str, object], list[str]]:
    """迁移 v1.4/v1.5 的 connect 旧结构。"""
    rules_applied: list[str] = []
    if not any(key in connect_payload for key in ("http", "ws", "reverseWs")):
        return connect_payload, rules_applied

    network_defaults = _extract_legacy_network_defaults(bot_payload, advanced_payload)
    http_payload = _ensure_dict(connect_payload.get("http"))
    ws_payload = _ensure_dict(connect_payload.get("ws"))
    reverse_ws_payload = _ensure_dict(connect_payload.get("reverseWs"))

    migrated_connect = {
        "httpServers": [],
        "httpSseServers": [],
        "httpClients": [],
        "websocketServers": [],
        "websocketClients": [],
        "plugins": [],
    }

    http_server = _build_legacy_http_server(http_payload, network_defaults)
    if http_server is not None:
        migrated_connect["httpServers"].append(http_server)
        rules_applied.append("connect.http -> connect.httpServers[0]")

    http_clients = _build_legacy_http_clients(http_payload, network_defaults)
    if http_clients:
        migrated_connect["httpClients"].extend(http_clients)
        rules_applied.append("connect.http.postUrls -> connect.httpClients")

    websocket_server = _build_legacy_websocket_server(ws_payload, network_defaults)
    if websocket_server is not None:
        migrated_connect["websocketServers"].append(websocket_server)
        rules_applied.append("connect.ws -> connect.websocketServers[0]")

    websocket_clients = _build_legacy_websocket_clients(reverse_ws_payload, network_defaults)
    if websocket_clients:
        migrated_connect["websocketClients"].extend(websocket_clients)
        rules_applied.append("connect.reverseWs -> connect.websocketClients")

    return migrated_connect, rules_applied


def _ensure_current_connect_shape(connect_payload: dict[str, object]) -> tuple[dict[str, object], list[str]]:
    """补齐当前 connect 结构缺失的列表字段。"""
    rules_applied: list[str] = []
    normalized = _ensure_dict(connect_payload)
    for key in (
        "httpServers",
        "httpSseServers",
        "httpClients",
        "websocketServers",
        "websocketClients",
        "plugins",
    ):
        if key not in normalized or not isinstance(normalized.get(key), list):
            normalized[key] = []
            rules_applied.append(f"connect.{key} default")
    return normalized, rules_applied


def _migrate_legacy_advanced_fields(
    advanced_payload: dict[str, object], bot_payload: dict[str, object]
) -> tuple[dict[str, object], list[str]]:
    """迁移旧版 advanced 字段命名和默认值。"""
    rules_applied: list[str] = []
    normalized = _ensure_dict(advanced_payload)
    legacy_enable_local_file_to_url = normalized.pop("enableLocalFile2Url", None)
    legacy_enable_local_file_to_url_level = (
        legacy_enable_local_file_to_url if isinstance(legacy_enable_local_file_to_url, str) else None
    )

    if "offlineNotice" not in normalized and "offline_notice" in normalized:
        normalized["offlineNotice"] = _normalize_bool(normalized.pop("offline_notice"), False)
        rules_applied.append("advanced.offline_notice -> advanced.offlineNotice")

    legacy_local_file_to_url = normalized.pop("localFile2url", None)
    if isinstance(legacy_enable_local_file_to_url, bool):
        normalized["enableLocalFile2Url"] = legacy_enable_local_file_to_url
    elif isinstance(legacy_local_file_to_url, bool):
        if "enableLocalFile2Url" not in normalized:
            normalized["enableLocalFile2Url"] = legacy_local_file_to_url
            rules_applied.append("advanced.localFile2url -> advanced.enableLocalFile2Url")
    elif isinstance(legacy_enable_local_file_to_url, str):
        normalized["enableLocalFile2Url"] = _normalize_bool(legacy_enable_local_file_to_url, False)
        rules_applied.append("advanced.enableLocalFile2Url normalized")
    else:
        normalized["enableLocalFile2Url"] = False
        rules_applied.append("advanced.enableLocalFile2Url default")

    if "parseMultMsg" not in normalized and "parseMultMsg" in bot_payload:
        normalized["parseMultMsg"] = _normalize_bool(bot_payload.pop("parseMultMsg"), False)
        rules_applied.append("bot.parseMultMsg -> advanced.parseMultMsg")
    elif "parseMultMsg" not in normalized:
        normalized["parseMultMsg"] = False
        rules_applied.append("advanced.parseMultMsg default")

    if "packetBackend" not in normalized:
        normalized["packetBackend"] = "auto"
        rules_applied.append("advanced.packetBackend default")

    if "fileLogLevel" not in normalized:
        normalized["fileLogLevel"] = _normalize_log_level(
            legacy_enable_local_file_to_url_level,
            default="debug",
        )
        rules_applied.append("advanced.fileLogLevel normalized")

    if "consoleLogLevel" not in normalized:
        normalized["consoleLogLevel"] = _normalize_log_level(normalized.get("consoleLogLevel"), default="info")
        rules_applied.append("advanced.consoleLogLevel default")

    if "consoleLog" not in normalized:
        normalized["consoleLog"] = True
        rules_applied.append("advanced.consoleLog default")

    if "fileLog" not in normalized:
        normalized["fileLog"] = False
        rules_applied.append("advanced.fileLog default")

    if "o3HookMode" not in normalized:
        normalized["o3HookMode"] = 1
        rules_applied.append("advanced.o3HookMode default")

    if "offlineNotice" not in normalized:
        normalized["offlineNotice"] = False
        rules_applied.append("advanced.offlineNotice default")

    if "autoStart" not in normalized:
        normalized["autoStart"] = False
        rules_applied.append("advanced.autoStart default")

    if "packetServer" not in normalized:
        normalized["packetServer"] = ""
        rules_applied.append("advanced.packetServer default")

    return normalized, rules_applied


def _migrate_legacy_bot_fields(bot_payload: dict[str, object]) -> tuple[dict[str, object], list[str]]:
    """补齐 bot 域的新增字段。"""
    rules_applied: list[str] = []
    normalized = _ensure_dict(bot_payload)

    normalized.setdefault("musicSignUrl", "")

    if "autoRestartSchedule" not in normalized:
        normalized["autoRestartSchedule"] = _clone_payload(DEFAULT_AUTO_RESTART_SCHEDULE_PAYLOAD)
        rules_applied.append("bot.autoRestartSchedule default")

    if "offlineAutoRestart" not in normalized:
        normalized["offlineAutoRestart"] = False
        rules_applied.append("bot.offlineAutoRestart default")

    return normalized, rules_applied


def _migrate_bot_entry_payload(payload: object) -> tuple[dict[str, object], list[str]]:
    """将旧版单个 Bot 配置迁移为当前结构。"""
    if not isinstance(payload, dict):
        raise TypeError("单个 Bot 配置必须为对象")

    migrated = _clone_payload(payload)
    bot_payload = _ensure_dict(migrated.get("bot"))
    advanced_payload = _ensure_dict(migrated.get("advanced"))
    connect_payload = _ensure_dict(migrated.get("connect"))

    rules_applied: list[str] = []

    advanced_payload, advanced_rules = _migrate_legacy_advanced_fields(advanced_payload, bot_payload)
    rules_applied.extend(advanced_rules)

    connect_payload, connect_rules = _migrate_legacy_connect_shape(connect_payload, bot_payload, advanced_payload)
    rules_applied.extend(connect_rules)
    connect_payload, connect_shape_rules = _ensure_current_connect_shape(connect_payload)
    rules_applied.extend(connect_shape_rules)

    bot_payload, bot_rules = _migrate_legacy_bot_fields(bot_payload)
    rules_applied.extend(bot_rules)

    migrated["bot"] = bot_payload
    migrated["connect"] = connect_payload
    migrated["advanced"] = advanced_payload

    return migrated, rules_applied


class AutoRestartScheduleConfig(BaseModel):
    """自动重启计划配置"""

    enable: bool = Field(default=False, description="是否启用自动重启计划任务")
    time_unit: TimeUnitEnum = Field(
        default=TimeUnitEnum.HOUR,
        description="时间单位, m: 分钟, h: 小时, d: 天, mon: 月, year: 年",
    )
    duration: int = Field(default=6, description="时间长度, 仅包含数字, 不包含单位")

    @model_validator(mode="before")
    @classmethod
    def normalize_legacy_schedule(cls, data: Any) -> Any:
        """兼容旧版 interval/taskType 结构。"""
        if not isinstance(data, dict):
            return data

        if "time_unit" in data and "duration" in data:
            return data

        legacy_keys = {"taskType", "interval", "crontab", "jitter"}
        if not legacy_keys.intersection(data):
            return data

        normalized_interval = _parse_legacy_auto_restart_interval(data.get("interval"))
        time_unit = normalized_interval[0] if normalized_interval else TimeUnitEnum.HOUR
        duration = normalized_interval[1] if normalized_interval else 6

        task_type = str(data.get("taskType", "")).strip().lower()
        if task_type in {"cron", "crontab"}:
            enable = False
        elif task_type in {"interval"}:
            enable = bool(data.get("enable", True))
        elif task_type in {"none", "disabled", "off", "false", "0"}:
            enable = False
        else:
            enable = bool(data.get("enable", False))

        return {
            "enable": enable,
            "time_unit": time_unit.value,
            "duration": duration,
        }


class BotConfig(BaseModel):
    name: str
    QQID: str | int
    musicSignUrl: str = ""
    autoRestartSchedule: AutoRestartScheduleConfig = Field(default_factory=AutoRestartScheduleConfig)
    offlineAutoRestart: bool = False

    @field_validator("name")
    @staticmethod
    def validate_name(value: str) -> str:
        """验证机器人名称。"""
        if not value:
            return "".join(random.choices(string.ascii_letters, k=8))
        return value

    @field_validator("QQID")
    @staticmethod
    def validate_qqid(value: str | int) -> int:
        """验证 QQID 并统一为 int。"""
        if isinstance(value, int):
            return value
        if isinstance(value, str):
            try:
                return int(value)
            except ValueError as error:
                raise ValueError(f"QQ号 '{value}' 无法转换为整数") from error
        raise TypeError("QQ号必须是字符串或整数")


class NetworkBaseConfig(BaseModel):
    enable: bool = True
    name: str
    messagePostFormat: Literal["array", "string"] = "array"
    token: str = ""
    debug: bool = False


class HttpServersConfig(NetworkBaseConfig):
    host: str
    port: int
    enableCors: bool = False
    enableWebsocket: bool = False


class HttpSseServersConfig(NetworkBaseConfig):
    host: str
    port: int
    enableCors: bool = False
    enableWebsocket: bool = False
    reportSelfMessage: bool = False


class HttpClientsConfig(NetworkBaseConfig):
    url: HttpUrl
    reportSelfMessage: bool = False


class WebsocketServersConfig(NetworkBaseConfig):
    host: str
    port: int
    reportSelfMessage: bool = False
    enableForcePushEvent: bool = False
    heartInterval: int = 30000

    @field_validator("heartInterval", mode="before")
    @classmethod
    def _coerce_heart_interval(cls, value):
        return _coerce_interval_default(value, 30000)


class WebsocketClientsConfig(NetworkBaseConfig):
    url: WebsocketUrl
    reportSelfMessage: bool = False
    heartInterval: int = 30000
    reconnectInterval: int = 30000

    @field_validator("heartInterval", "reconnectInterval", mode="before")
    @classmethod
    def _coerce_client_intervals(cls, value):
        return _coerce_interval_default(value, 30000)


class ConnectConfig(BaseModel):
    httpServers: list[HttpServersConfig] = Field(default_factory=list)
    httpSseServers: list[HttpSseServersConfig] = Field(default_factory=list)
    httpClients: list[HttpClientsConfig] = Field(default_factory=list)
    websocketServers: list[WebsocketServersConfig] = Field(default_factory=list)
    websocketClients: list[WebsocketClientsConfig] = Field(default_factory=list)
    plugins: list = Field(default_factory=list)


class AdvancedConfig(BaseModel):
    autoStart: bool = False
    offlineNotice: bool = False
    parseMultMsg: bool = False
    packetServer: str = ""
    packetBackend: str = Field(default="auto", exclude=True)
    enableLocalFile2Url: bool = False
    fileLog: bool = False
    consoleLog: bool = True
    fileLogLevel: Literal["debug", "info", "error"] = "debug"
    consoleLogLevel: Literal["debug", "info", "error"] = "info"
    o3HookMode: Literal[0, 1] = 1


class Config(BaseModel):
    bot: BotConfig
    connect: ConnectConfig
    advanced: AdvancedConfig

    @model_validator(mode="before")
    @classmethod
    def normalize_legacy_payload(cls, data: Any) -> Any:
        """兼容旧版单个 Bot 配置结构。"""
        if not isinstance(data, dict):
            return data
        migrated, _ = _migrate_bot_entry_payload(data)
        return migrated


class ConfigInfo(BaseModel):
    """Bot 配置文件元信息。"""

    configVersion: str = BOT_CONFIG_COMPAT_VERSION


class ConfigCollection(BaseModel):
    """Bot 配置文件根结构。"""

    info: ConfigInfo = Field(default_factory=ConfigInfo)
    bots: list[Config] = Field(default_factory=list)


class OneBotConfig(BaseModel):
    network: ConnectConfig
    musicSignUrl: str = ""
    enableLocalFile2Url: bool = False
    parseMultMsg: bool = False


class NapCatConfig(BaseModel):
    fileLog: bool
    consoleLog: bool
    fileLogLevel: str
    consoleLogLevel: str
    packetBackend: str = Field(default="auto", exclude=True)
    packetServer: str
    o3HookMode: Literal[0, 1] = 1


def _coerce_interval_default(value, default: int = 30000) -> int:
    """将可能来自配置的间隔值规范化为整数，无法解析时返回默认值。"""
    if value is None:
        return default
    if isinstance(value, str):
        normalized = value.strip()
        if normalized == "":
            return default
        try:
            return int(normalized)
        except ValueError:
            return default
    try:
        return int(value)
    except (TypeError, ValueError):
        return default


def _parse_legacy_auto_restart_interval(value: Any) -> tuple[TimeUnitEnum, int] | None:
    """解析旧版自动重启间隔字符串。"""
    if isinstance(value, int) and value > 0:
        return TimeUnitEnum.HOUR, value
    if not isinstance(value, str):
        return None

    match = _LEGACY_AUTO_RESTART_INTERVAL_PATTERN.fullmatch(value.strip())
    if match is None:
        return None

    duration = int(match.group(1))
    if duration <= 0:
        return None

    return TimeUnitEnum(match.group(2).lower()), duration


def read_bot_config_version(payload: object) -> str:
    """读取 bot.json 的配置兼容版本。"""
    if isinstance(payload, dict):
        info = _ensure_dict(payload.get("info"))
        version = info.get("configVersion")
        if isinstance(version, str) and version.strip():
            return version.strip()
    return BOT_CONFIG_LEGACY_VERSION


def migrate_bot_config_payload(payload: object) -> tuple[dict[str, object], str, list[str]]:
    """将 bot.json 迁移到当前根结构。"""
    current_version = read_bot_config_version(payload)
    rules_applied: list[str] = []

    if isinstance(payload, list):
        raw_bots = payload
        rules_applied.append("root list -> object with info/bots")
    elif isinstance(payload, dict):
        if "bots" in payload:
            raw_bots_value = payload.get("bots")
            if not isinstance(raw_bots_value, list):
                raise TypeError("bot.json 的 bots 字段必须为列表")
            raw_bots = raw_bots_value
        elif {"bot", "connect", "advanced"}.issubset(payload):
            raw_bots = [payload]
            rules_applied.append("single bot object -> object with info/bots")
        else:
            raise TypeError("bot.json 根节点必须为列表、包含 bots 的对象或单个 Bot 对象")
    else:
        raise TypeError("bot.json 根节点必须为列表或对象")

    migrated_bots: list[dict[str, object]] = []
    for index, raw_bot in enumerate(raw_bots):
        migrated_bot, entry_rules = _migrate_bot_entry_payload(raw_bot)
        if entry_rules:
            rules_applied.extend(f"bots[{index}]: {rule}" for rule in entry_rules)
        migrated_bots.append(migrated_bot)

    migrated_payload = {
        "info": {
            "configVersion": BOT_CONFIG_COMPAT_VERSION,
        },
        "bots": migrated_bots,
    }
    return migrated_payload, current_version, rules_applied


def serialize_bot_config_collection(configs: list[Config], version: str = BOT_CONFIG_COMPAT_VERSION) -> dict[str, object]:
    """将 Bot 配置列表序列化为当前根结构。"""
    return {
        "info": {
            "configVersion": version,
        },
        "bots": [json_payload(config) for config in configs],
    }


def json_payload(model: BaseModel) -> Any:
    """将模型序列化为 JSON 兼容结构。"""
    import json

    return json.loads(model.model_dump_json())
