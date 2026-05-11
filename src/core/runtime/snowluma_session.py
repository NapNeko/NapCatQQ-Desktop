# -*- coding: utf-8 -*-
"""SnowLuma WebUI 密码 session 持久化 (Tier G, P2 注入流程自动化).

Desktop 单向主导密码: 用户**不能**在 SnowLuma WebUI 改密 (改了会被 Desktop 在
下次启动 Bot 时覆盖回 ``session.json`` 里的密码).

文件位置: ``<runtime_path>/config/snowluma-session.json`` (Desktop 侧, 与 SnowLuma
config 解耦; 即使用户重装 SnowLuma, Desktop 仍能识别上次密码).

权限: Windows 下 ``os.chmod(0o600)``, 仅当前用户 ACL 可读写.

Schema::

    {
      "password": "<随机强密码 >=10 字符 + 大小写 + 特殊符号 + 不含空格>",
      "created_at": "<ISO 8601 UTC, 含 Z 后缀>",
      "last_rendered_at": "<ISO 8601 UTC>"
    }

参见: ``docs/requirements/2026-05-10-snowluma-bot-form-backend-aware.md`` §2.12 / §10.2.
"""
from __future__ import annotations

import json
import os
import secrets
import string
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path

from src.core.logging import LogSource, LogType, logger

# W5: ``resolve_effective_password`` 签名重写, 不再依赖 :class:`Config`; 移除 TYPE_CHECKING.


@dataclass(frozen=True)
class SnowLumaSession:
    """Desktop 侧 SnowLuma WebUI 密码 session.

    Attributes:
        password: 明文密码 (写入 ``snowluma-session.json``; 仅当前用户可读).
        created_at: ISO 8601 UTC 时间戳; 首次生成后 sticky, 不随 ``last_rendered_at`` 变化.
        last_rendered_at: 上一次 ``render_webui_json(password=...)`` 的时间戳;
            sticky 与否取决于实际是否覆写了 ``webui.json``.
    """

    password: str
    created_at: str
    last_rendered_at: str


def session_path() -> Path:
    """返回 Desktop 侧 ``snowluma-session.json`` 绝对路径.

    锚定 ``PathFunc.config_dir_path`` (即 ``runtime/config/``), 与 ``.gitignore``
    + ``collection_filters._filter_snowluma_session`` 配合, 防止密码明文进 git
    或打包产物.
    """
    # 延迟导入避免在测试 fixture 之类不需要 PathFunc 的场景下也加载 creart 单例
    from creart import it
    from src.core.runtime.paths import PathFunc

    return it(PathFunc).config_dir_path / "snowluma-session.json"


def load_session() -> SnowLumaSession | None:
    """读 ``snowluma-session.json``.

    - 不存在 / JSON 解析失败 / 必填字段缺失 / 字段为空 → 返回 ``None``;
      调用方按"首次场景"处理 (即调 :func:`create_session` 重生).

    Returns:
        :class:`SnowLumaSession` or ``None``.
    """
    target = session_path()
    if not target.exists():
        return None
    try:
        payload = json.loads(target.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        logger.warning(
            f"snowluma-session.json 损坏, 视为不存在: {target}",
            LogType.FILE_FUNC,
            LogSource.CORE,
        )
        return None
    if not isinstance(payload, dict):
        return None

    password = payload.get("password")
    created_at = payload.get("created_at")
    last_rendered_at = payload.get("last_rendered_at")
    if not all(isinstance(v, str) and v for v in (password, created_at, last_rendered_at)):
        return None

    return SnowLumaSession(
        password=password,  # type: ignore[arg-type]
        created_at=created_at,  # type: ignore[arg-type]
        last_rendered_at=last_rendered_at,  # type: ignore[arg-type]
    )


def create_session() -> SnowLumaSession:
    """生成新的强密码 + 写 ``snowluma-session.json`` + ``chmod 0o600``.

    幂等: 调用者负责检查 ``load_session() is None`` 后再调本函数, 否则会覆盖已有密码.
    """
    target = session_path()
    target.parent.mkdir(parents=True, exist_ok=True)
    password = _generate_strong_password()
    now_iso = _utc_now_iso()
    session = SnowLumaSession(password=password, created_at=now_iso, last_rendered_at=now_iso)
    _write_session(target, session)
    return session


def resolve_effective_password(*, override: str = "") -> str:
    """解析 SnowLuma WebUI 的**有效**密码 (W5 签名重写: 接 App 级 override 字符串).

    W5 之前: 接 ``BotConfig`` 读 ``bot.snowluma_webui_password_override`` (per-Bot).
    W5 之后: ``snowluma_webui_password_override`` 已迁移到 App 级 QConfig (
    ``cfg.snowluma_webui_password_override``); 本函数纯粹处理 ``override`` 字符串与
    session.json 的回退, 不再耦合 BotConfig / cfg. 调用方 (一般是
    :class:`SnowLumaDaemon`) 负责从 cfg 读出当前值后传入.

    优先级:

    1. ``override`` 非空 (strip 后) → 直接返回该值, 作为 webui.json + login HTTP
       的唯一权威.
    2. 否则 → :func:`load_session` 读 ``snowluma-session.json``; 不存在则
       :func:`create_session` 现场生成强密码落盘.

    Args:
        override: App 级密码 override (一般来自
            ``cfg.get(cfg.snowluma_webui_password_override)``). 空字符串等价于"未设置".

    Returns:
        生效的 WebUI 明文密码 (非空字符串).
    """
    cleaned = (override or "").strip()
    if cleaned:
        return cleaned
    session = load_session()
    if session is None:
        session = create_session()
    return session.password


def update_last_rendered(session: SnowLumaSession) -> SnowLumaSession:
    """更新 ``last_rendered_at`` 字段后落盘, 返回新 session 对象.

    一般在 :func:`render_webui_json` 之后调一次, 让 ``snowluma-session.json`` 与
    ``webui.json`` 的时间戳保持同步.
    """
    new_session = SnowLumaSession(
        password=session.password,
        created_at=session.created_at,
        last_rendered_at=_utc_now_iso(),
    )
    _write_session(session_path(), new_session)
    return new_session


# ==================== 内部 ====================
def _write_session(target: Path, session: SnowLumaSession) -> None:
    payload = {
        "password": session.password,
        "created_at": session.created_at,
        "last_rendered_at": session.last_rendered_at,
    }
    target.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )
    try:
        os.chmod(target, 0o600)
    except OSError:
        # Windows 上 chmod 0o600 在 ACL 模型下语义不严格, 但仍尝试.
        # 实际安全性依赖 .gitignore + collection_filters._filter_snowluma_session.
        pass


def _generate_strong_password(length: int = 16) -> str:
    """满足 SnowLuma ``webui/auth.ts:38-44`` 强密码规则:
    >=10 字符 + 含大写 + 含小写 + 含特殊符号 + 不含空格.

    Args:
        length: 最终密码长度; ``< 10`` 会被 floor 到 10.

    Returns:
        随机强密码字符串.
    """
    if length < 10:
        length = 10
    upper = string.ascii_uppercase
    lower = string.ascii_lowercase
    digits = string.digits
    # SnowLuma 强密码正则要求至少 1 个非字母数字非空格字符; 这里给一个保守集合,
    # 避免 shell / JSON 转义边界字符 (`'\"\\` 等) 出现在密码里.
    specials = "!@#$%^&*()-_=+[]{};:,.<>/?"
    # 保证每类至少 1 个
    seed = [
        secrets.choice(upper),
        secrets.choice(lower),
        secrets.choice(digits),
        secrets.choice(specials),
    ]
    pool = upper + lower + digits + specials
    seed.extend(secrets.choice(pool) for _ in range(length - len(seed)))
    secrets.SystemRandom().shuffle(seed)
    return "".join(seed)


def _utc_now_iso() -> str:
    """生成 ISO 8601 UTC 时间戳, 格式 ``YYYY-MM-DDTHH:MM:SS.mmmZ``.

    与 :func:`render_webui_json` 内部 ``_utc_now_iso`` 保持一致, 避免 SnowLuma 上游
    解析两个文件时格式不匹配.
    """
    now = datetime.now(timezone.utc)
    millis = now.microsecond // 1000
    return now.strftime("%Y-%m-%dT%H:%M:%S") + f".{millis:03d}Z"
