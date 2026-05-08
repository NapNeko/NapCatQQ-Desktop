# -*- coding: utf-8 -*-
# 标准库导入
import json
import os
import platform
import re
import sys
import zipfile
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any
from urllib.parse import urlsplit

from PySide6.QtCore import QStandardPaths

# 项目内模块导入
from src.core.network.urls import Urls
from src.core.platform.app_paths import resolve_app_base_path, resolve_app_data_path

REDACTED_EMAIL = "<redacted-email>"
REDACTED_SECRET = "<redacted-secret>"
REDACTED_URL = "<redacted-url>"

_SENSITIVE_KEY_PATTERN = re.compile(
    r"(?i)([\"']?(?:token|secret|password|passwd|auth|authorization|credential|cookie|api[_-]?key|apikey|bearer)"
    r"[\"']?\s*[:=]\s*)([\"']?)([^\"',\s}\]]+)([\"']?)"
)
_QQID_KEY_PATTERN = re.compile(r"(?i)([\"']?(?:QQID|bot_qq_id|qq_id)[\"']?\s*[:=]\s*)([\"']?)(\d{5,20})([\"']?)")
_QQID_LABEL_PATTERN = re.compile(r"(?i)(\bQQID\s*[:=]\s*)(\d{5,20})")

# P5 F3.1: QQID 路径 / 命令行 / 中括号形式的窄正则补全.
# 远程链路实际打到 app.log 的形态历史脱敏正则全部漏网, 这里逐个对应:
#   - ``napcat_<qqid>.log`` / ``.json`` / ``.pid`` / ``.log.prev``
#   - ``onebot11_<qqid>.json``
#   - ``qq --no-sandbox -q <qqid>``
#   - ``ManagerNapCatQQProcess[<qqid>]``
# 替换函数仅触碰捕获组里的数字段, 前后字面量原样保留.
_QQID_FILENAME_PATTERN = re.compile(
    r"((?:napcat|onebot11)_)(\d{5,12})(\.(?:log|json|pid)(?:\.prev)?)\b"
)
_QQID_CMDLINE_Q_PATTERN = re.compile(r"(\B-q\s+|\b-q\s+)(\d{5,12})\b")
_QQID_BRACKET_PATTERN = re.compile(
    r"(ManagerNapCatQQProcess\[)(\d{5,12})(\])"
)

_AUTHORIZATION_BEARER_PATTERN = re.compile(r"(?i)(authorization\s*[:=]\s*bearer\s+)(\S+)")
_URL_TOKEN_PATTERN = re.compile(r"(?i)([?&]token=)([^&\s]+)")
_EMAIL_PATTERN = re.compile(r"(?i)\b[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}\b")
_URL_PATTERN = re.compile(r"(?i)\b(?:https?|wss?)://[^\s\"'<>]+")

# P5 F3.2: SSH 主机 / 用户名 / 隧道 label 的导出脱敏.
# TRCE 实时落盘保留原始字符串便于排错; 仅在导出诊断包时统一脱敏.
# - ``host=ac.rainplay.cn`` / ``hostname=10.0.0.5``: 仅保留首字符 + 顶级域(或 IP 末段)
# - ``username=root``: 仅保留首字符
# - ``label=<host>->127.0.0.1:6099``: SSH 隧道 label 把箭头左边的 host 段脱敏
#
# 设计要点: ``\b`` 边界 + 显式 ``host`` / ``hostname`` / ``username`` 词项,
# 防止 ``hosting=`` / ``my_host_var`` 这类带子串的字段被误伤.
_HOST_KEY_PATTERN = re.compile(
    r"(?i)([\"']?(?:host|hostname)[\"']?\s*[:=]\s*)([\"']?)([A-Za-z0-9._\-]+)([\"']?)"
)
_USERNAME_KEY_PATTERN = re.compile(
    r"(?i)([\"']?username[\"']?\s*[:=]\s*)([\"']?)([A-Za-z0-9._\-]+)([\"']?)"
)
_TUNNEL_LABEL_PATTERN = re.compile(
    r"(?i)(\blabel\s*=\s*)([A-Za-z0-9._\-]+)(->)"
)


@dataclass(frozen=True)
class CrashBundlePayload:
    """崩溃诊断包载荷. """

    trigger: str
    created_at: datetime
    exception_type: str
    exception_message: str
    traceback_text: str
    log_path: Path
    base_path: Path


def mask_qqid(value: Any) -> str:
    """对 QQID 进行掩码处理. """
    digits = "".join(ch for ch in str(value) if ch.isdigit())
    if not digits:
        return "***"
    return f"***{digits[-4:]}"


def mask_host(value: Any) -> str:
    """对主机名 / IP 进行掩码处理 (P5 F3.2).

    - 含 ``.`` (域名 / IPv4): 保留首字符 + 最后一段, 例如:
      - ``ac.rainplay.cn`` -> ``a***.cn``
      - ``server.example.com`` -> ``s***.com``
      - ``10.0.0.5`` -> ``1***.5``
    - 不含 ``.`` (短主机名): 仅保留首字符 ``myserver`` -> ``m***``
    - 空值: ``<empty-host>``

    输出仍可让用户自己识别"哪台服务器", 但第三方拿到诊断包无法
    定向 SSH 暴力破解 / 端口扫描.
    """
    text = str(value or "").strip()
    if not text:
        return "<empty-host>"
    if "." in text:
        prefix = text[:1]
        suffix = text.rsplit(".", 1)[1]
        return f"{prefix}***.{suffix}"
    return f"{text[:1]}***"


def mask_username(value: Any) -> str:
    """对 SSH 用户名进行掩码处理 (P5 F3.2).

    保留首字符让用户自己识别 (``root`` -> ``r***``, ``alice_dev`` -> ``a***``);
    空值返回 ``<empty-user>``.
    """
    text = str(value or "").strip()
    if not text:
        return "<empty-user>"
    return f"{text[:1]}***"


def mask_email(value: Any) -> str:
    """对邮箱进行掩码处理. """
    text = str(value or "").strip()
    if "@" not in text:
        return REDACTED_EMAIL

    local_part, _, domain = text.partition("@")
    if not local_part or not domain:
        return REDACTED_EMAIL

    prefix = local_part[:1]
    return f"{prefix}***@{domain}"


def summarize_url(value: Any) -> str:
    """输出适合日志记录的 URL 摘要, 不包含 query 和 token. """
    text = str(value or "").strip()
    if not text:
        return "<empty-url>"

    parsed = urlsplit(text)
    if not parsed.scheme or not parsed.netloc:
        return "<invalid-url>"

    path = parsed.path.rstrip("/")
    tail = Path(path).name if path else ""
    suffix = f"/.../{tail}" if tail else ""
    return f"{parsed.scheme}://{parsed.netloc}{suffix}"


def summarize_path(value: Any) -> str:
    """输出适合日志记录的路径摘要, 仅保留末尾两级. """
    text = str(value or "").strip()
    if not text:
        return "<empty-path>"

    parts = Path(text).parts
    if len(parts) >= 2:
        return str(Path("...") / parts[-2] / parts[-1])
    return str(Path("...") / Path(text).name)


def sanitize_text_for_export(text: str) -> str:
    """对导出文本进行严格脱敏. """
    sanitized = text or ""

    sanitized = _AUTHORIZATION_BEARER_PATTERN.sub(r"\1" + REDACTED_SECRET, sanitized)
    sanitized = _URL_TOKEN_PATTERN.sub(r"\1" + REDACTED_SECRET, sanitized)

    def replace_sensitive_key(match: re.Match[str]) -> str:
        return f"{match.group(1)}{match.group(2)}{REDACTED_SECRET}{match.group(4)}"

    def replace_qqid_key(match: re.Match[str]) -> str:
        return f"{match.group(1)}{match.group(2)}{mask_qqid(match.group(3))}{match.group(4)}"

    sanitized = _SENSITIVE_KEY_PATTERN.sub(replace_sensitive_key, sanitized)
    sanitized = _QQID_KEY_PATTERN.sub(replace_qqid_key, sanitized)
    sanitized = _QQID_LABEL_PATTERN.sub(lambda m: f"{m.group(1)}{mask_qqid(m.group(2))}", sanitized)

    # P5 F3.1: 路径 / 命令行 / 中括号形式的窄正则补全, 仅替换数字段
    sanitized = _QQID_FILENAME_PATTERN.sub(
        lambda m: f"{m.group(1)}{mask_qqid(m.group(2))}{m.group(3)}", sanitized
    )
    sanitized = _QQID_CMDLINE_Q_PATTERN.sub(
        lambda m: f"{m.group(1)}{mask_qqid(m.group(2))}", sanitized
    )
    sanitized = _QQID_BRACKET_PATTERN.sub(
        lambda m: f"{m.group(1)}{mask_qqid(m.group(2))}{m.group(3)}", sanitized
    )

    # P5 F3.2: SSH host / username / tunnel label 脱敏
    # 顺序: tunnel_label 最先 (label= 优先于一般 host=), URL 模式留到最后兜底
    sanitized = _TUNNEL_LABEL_PATTERN.sub(
        lambda m: f"{m.group(1)}{mask_host(m.group(2))}{m.group(3)}", sanitized
    )
    sanitized = _HOST_KEY_PATTERN.sub(
        lambda m: f"{m.group(1)}{m.group(2)}{mask_host(m.group(3))}{m.group(4)}", sanitized
    )
    sanitized = _USERNAME_KEY_PATTERN.sub(
        lambda m: f"{m.group(1)}{m.group(2)}{mask_username(m.group(3))}{m.group(4)}", sanitized
    )

    sanitized = _EMAIL_PATTERN.sub(REDACTED_EMAIL, sanitized)
    sanitized = _URL_PATTERN.sub(REDACTED_URL, sanitized)
    return sanitized


def resolve_desktop_output_dir(base_path: Path | None = None) -> tuple[Path, str]:
    """解析桌面输出目录, 失败时回退到应用目录. """
    app_base_path = base_path or resolve_app_base_path()

    try:
        desktop = QStandardPaths.writableLocation(QStandardPaths.StandardLocation.DesktopLocation)
    except Exception:
        desktop = ""

    if desktop:
        desktop_path = Path(desktop)
        if desktop_path.exists() and desktop_path.is_dir() and os.access(desktop_path, os.W_OK):
            return desktop_path, "desktop"

    home_desktop_path = Path.home() / "Desktop"
    if home_desktop_path.exists() and home_desktop_path.is_dir() and os.access(home_desktop_path, os.W_OK):
        return home_desktop_path, "home_desktop_fallback"

    return app_base_path, "app_dir_fallback"


def build_safe_config_summary(
    config_path: Path | None = None,
    bot_config_path: Path | None = None,
) -> dict[str, Any]:
    """根据 allowlist 构建安全配置摘要. """
    runtime_root = resolve_app_data_path() / "runtime"
    runtime_config_path = config_path or runtime_root / "config" / "config.json"
    runtime_bot_config_path = bot_config_path or runtime_root / "config" / "bot.json"

    config_data = _load_json_file(runtime_config_path)
    bot_data = _load_json_file(runtime_bot_config_path)

    info_section = _as_dict(config_data.get("Info"))
    event_section = _as_dict(config_data.get("Event"))
    personalize_section = _as_dict(config_data.get("Personalize"))
    personalized_section = _as_dict(config_data.get("Personalized"))
    email_section = _as_dict(config_data.get("Email"))
    webhook_section = _as_dict(config_data.get("WebHook"))

    summary = {
        "global": {
            "EulaAccepted": info_section.get("EulaAccepted"),
            "MainWindow": info_section.get("MainWindow"),
            "NCDVersion": info_section.get("NCDVersion"),
            "PlatformType": info_section.get("PlatformType"),
            "SystemType": info_section.get("SystemType"),
            "DpiScale": personalized_section.get("DpiScale"),
            "ThemeMode": personalize_section.get("ThemeMode"),
            "WindowOpacity": personalize_section.get("WindowOpacity"),
            "BotOfflineEmailNotice": event_section.get("BotOfflineEmailNotice"),
            "BotOfflineWebHookNotice": event_section.get("BotOfflineWebHookNotice"),
        },
        "email": {
            "configured": any(
                bool(email_section.get(key))
                for key in ("EmailReceiver", "EmailSender", "EmailToken", "EmailStmpServer")
            ),
            "encryption": email_section.get("EmailEncryption"),
            "smtp_port": email_section.get("EmailStmpPort"),
            "smtp_server_configured": bool(email_section.get("EmailStmpServer")),
        },
        "webhook": {
            "configured": any(bool(webhook_section.get(key)) for key in ("WebHookUrl", "WebHookSecret", "WebHookJson")),
            "json_configured": bool(webhook_section.get("WebHookJson")),
            "secret_configured": bool(webhook_section.get("WebHookSecret")),
        },
        "bots": [],
    }

    if isinstance(bot_data, list):
        for index, bot_entry in enumerate(bot_data):
            bot_entry_dict = _as_dict(bot_entry)
            bot_section = _as_dict(bot_entry_dict.get("bot"))
            connect_section = _as_dict(bot_entry_dict.get("connect"))
            advanced_section = _as_dict(bot_entry_dict.get("advanced"))

            summary["bots"].append(
                {
                    "bot_index": index,
                    "qqid_masked": mask_qqid(bot_section.get("QQID")),
                    "auto_restart_enabled": bool(_as_dict(bot_section.get("autoRestartSchedule")).get("enable")),
                    "offline_auto_restart": bool(bot_section.get("offlineAutoRestart")),
                    "connect_counts": {
                        "httpServers": len(_as_list(connect_section.get("httpServers"))),
                        "httpSseServers": len(_as_list(connect_section.get("httpSseServers"))),
                        "httpClients": len(_as_list(connect_section.get("httpClients"))),
                        "websocketServers": len(_as_list(connect_section.get("websocketServers"))),
                        "websocketClients": len(_as_list(connect_section.get("websocketClients"))),
                        "plugins": len(_as_list(connect_section.get("plugins"))),
                    },
                    "advanced_flags": {
                        "autoStart": bool(advanced_section.get("autoStart")),
                        "offlineNotice": bool(advanced_section.get("offlineNotice")),
                        "parseMultMsg": bool(advanced_section.get("parseMultMsg")),
                        "enableLocalFile2Url": bool(advanced_section.get("enableLocalFile2Url")),
                        "fileLog": bool(advanced_section.get("fileLog")),
                        "consoleLog": bool(advanced_section.get("consoleLog")),
                        "o3HookMode": advanced_section.get("o3HookMode"),
                    },
                    "log_level_flags": {
                        "fileLogLevel": advanced_section.get("fileLogLevel"),
                        "consoleLogLevel": advanced_section.get("consoleLogLevel"),
                    },
                }
            )

    return summary


def build_crash_bundle(output_dir: Path, payload: CrashBundlePayload) -> Path:
    """生成脱敏后的崩溃诊断包. """
    output_dir.mkdir(parents=True, exist_ok=True)

    base_name = f"NapCatQQ-Desktop-crash-{payload.created_at.strftime('%Y%m%d-%H%M%S-%f')}"
    archive_path = output_dir / f"{base_name}.zip"
    suffix = 1
    while archive_path.exists():
        archive_path = output_dir / f"{base_name}-{suffix}.zip"
        suffix += 1

    app_log_text = ""
    if payload.log_path.exists():
        app_log_text = payload.log_path.read_text(encoding="utf-8", errors="replace")

    crash_report = _build_crash_report(payload)
    app_meta = _build_app_meta(payload.base_path, payload.log_path)
    paths_snapshot = _build_paths_snapshot(payload.base_path, payload.log_path, output_dir)
    runtime_root = resolve_app_data_path() / "runtime"
    config_summary = build_safe_config_summary(
        config_path=runtime_root / "config" / "config.json",
        bot_config_path=runtime_root / "config" / "bot.json",
    )

    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("crash_report.txt", crash_report)
        archive.writestr("logs/app-session.redacted.log", sanitize_text_for_export(app_log_text))
        archive.writestr("diagnostics/app_meta.json", json.dumps(app_meta, ensure_ascii=False, indent=2))
        archive.writestr("diagnostics/config_summary.json", json.dumps(config_summary, ensure_ascii=False, indent=2))
        archive.writestr("diagnostics/paths.json", json.dumps(paths_snapshot, ensure_ascii=False, indent=2))

    return archive_path


def _build_crash_report(payload: CrashBundlePayload) -> str:
    """构建崩溃摘要文本. """
    sanitized_message = sanitize_text_for_export(payload.exception_message)
    sanitized_traceback = sanitize_text_for_export(payload.traceback_text)
    issue_url = Urls.NCD_ISSUES.value.toString()

    lines = [
        "NapCatQQ Desktop Crash Report",
        f"Created At: {payload.created_at.isoformat()}",
        f"Trigger: {payload.trigger}",
        f"Exception Type: {payload.exception_type or 'unknown'}",
        f"Exception Message: {sanitized_message or '(empty)'}",
        f"Current App Log: {payload.log_path}",
        f"Issue URL: {issue_url}",
        "",
        "Traceback:",
        sanitized_traceback or "(no traceback available)",
    ]
    return "\n".join(lines) + "\n"


def _build_app_meta(base_path: Path, log_path: Path) -> dict[str, Any]:
    """构建应用元信息. """
    data_path = resolve_app_data_path()
    config_data = _load_json_file(data_path / "runtime" / "config" / "config.json")
    info_section = _as_dict(config_data.get("Info"))

    return {
        "app_name": "NapCatQQ-Desktop",
        "app_version": info_section.get("NCDVersion"),
        "system_type": info_section.get("SystemType") or platform.system(),
        "platform_type": info_section.get("PlatformType") or platform.machine(),
        "python_version": platform.python_version(),
        "runtime_mode": "frozen" if getattr(sys, "frozen", False) else "source",
        "pid": os.getpid(),
        "cwd": str(Path.cwd()),
        "base_path": str(base_path),
        "data_path": str(data_path),
        "log_path": str(log_path),
        "start_time": info_section.get("StartTime"),
        "executable": sys.executable,
    }


def _build_paths_snapshot(base_path: Path, log_path: Path, output_dir: Path) -> dict[str, Any]:
    """构建关键路径快照. """
    data_path = resolve_app_data_path()
    paths = {
        "base_path": base_path,
        "data_path": data_path,
        "runtime_path": data_path / "runtime",
        "config_dir_path": data_path / "runtime" / "config",
        "config_path": data_path / "runtime" / "config" / "config.json",
        "bot_config_path": data_path / "runtime" / "config" / "bot.json",
        "tmp_path": data_path / "runtime" / "tmp",
        "napcat_path": data_path / "runtime" / "NapCatQQ",
        "log_dir": log_path.parent,
        "current_log_path": log_path,
        "bundle_output_dir": output_dir,
    }

    return {
        name: {
            "path": str(path),
            "exists": path.exists(),
        }
        for name, path in paths.items()
    }


def _load_json_file(path: Path) -> Any:
    """读取 JSON 文件, 失败时返回空字典. """
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (FileNotFoundError, json.JSONDecodeError, OSError):
        return {}


def _as_dict(value: Any) -> dict[str, Any]:
    """确保值为 dict. """
    return value if isinstance(value, dict) else {}


def _as_list(value: Any) -> list[Any]:
    """确保值为 list. """
    return value if isinstance(value, list) else []
