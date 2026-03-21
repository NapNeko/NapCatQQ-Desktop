# -*- coding: utf-8 -*-
from __future__ import annotations

import json
import re
from dataclasses import dataclass
from time import time

from creart import it
from PySide6.QtCore import QObject, QThreadPool, Signal

from src.core.config import cfg
from src.core.config.operate_config import read_config
from src.core.utils.get_version import GetLocalVersionRunnable, GetRemoteVersionRunnable, VersionData
from src.core.utils.run_napcat import ManagerNapCatQQLoginState, ManagerNapCatQQProcess
from src.ui.components.notice_timeline import (
    NoticeDismissMode,
    NoticeTimelineItemData,
    NoticeTimelineSectionData,
    NoticeTimelineStatus,
)


@dataclass(slots=True)
class _RuntimeNotice:
    key: str
    text: str
    status: NoticeTimelineStatus
    dismiss_mode: NoticeDismissMode


class HomeNoticeService(QObject):
    """首页通知聚合服务。"""

    sectionsChanged = Signal(object)

    _MAX_RUNTIME_NOTICES = 6
    _SNOOZE_SECONDS = 30 * 60

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._runtime_notices: list[_RuntimeNotice] = []
        self._debug_sections: list[NoticeTimelineSectionData] = []
        self._dismissed_session_keys: set[str] = set()
        self._local_versions = self._load_local_versions()
        self._remote_versions: VersionData | None = None
        self._remote_version_task: GetRemoteVersionRunnable | None = None

        self._process_manager = it(ManagerNapCatQQProcess)
        self._login_state_manager = it(ManagerNapCatQQLoginState)

        self._bind_signals()

    def refresh(self) -> None:
        self._local_versions = self._load_local_versions()
        self._cleanup_expired_snoozes()
        self._emit_sections()
        self._request_remote_versions()

    def dismiss_notice(self, item: NoticeTimelineItemData) -> None:
        if item.dismiss_mode == NoticeDismissMode.NONE:
            return

        if self._dismiss_debug_item(item.key):
            self._emit_sections()
            return

        if item.dismiss_mode == NoticeDismissMode.SESSION:
            self._dismissed_session_keys.add(item.key)
        elif item.dismiss_mode == NoticeDismissMode.PERSISTENT:
            ignored_keys = self._load_ignored_keys()
            ignored_keys.add(item.key)
            self._save_ignored_keys(ignored_keys)
        elif item.dismiss_mode == NoticeDismissMode.SNOOZE:
            snoozed_items = self._load_snoozed_items()
            snoozed_items[item.key] = int(time() + self._SNOOZE_SECONDS)
            self._save_snoozed_items(snoozed_items)

        self._runtime_notices = [notice for notice in self._runtime_notices if notice.key != item.key]
        self._emit_sections()

    def _bind_signals(self) -> None:
        self._process_manager.notification_signal.connect(self._on_core_notification)
        self._login_state_manager.notification_signal.connect(self._on_core_notification)
        self._login_state_manager.qr_code_available_signal.connect(self._on_qr_code_available)
        self._login_state_manager.qr_code_removed_signal.connect(self._on_qr_code_removed)
        home_notice_debug_center.sampleRequested.connect(self._show_debug_sections)
        home_notice_debug_center.clearRequested.connect(self._clear_debug_sections)

    @staticmethod
    def _load_local_versions() -> VersionData:
        return GetLocalVersionRunnable().execute()

    def _request_remote_versions(self) -> None:
        if self._remote_version_task is not None:
            return

        task = GetRemoteVersionRunnable()
        task.version_signal.connect(self._on_remote_versions_loaded)
        self._remote_version_task = task
        QThreadPool.globalInstance().start(task)

    def _on_remote_versions_loaded(self, version_data: VersionData) -> None:
        self._remote_versions = version_data
        self._remote_version_task = None
        self._emit_sections()

    def _on_core_notification(self, level: str, message: str) -> None:
        self._append_runtime_notice(
            key=f"runtime:{level}:{message}",
            text=message,
            status=self._status_from_level(level),
            dismiss_mode=NoticeDismissMode.SESSION,
        )
        self._emit_sections()

    def _on_qr_code_available(self, qq_id: str, qr_code: str) -> None:
        del qr_code
        self._append_runtime_notice(
            key=f"qr:{qq_id}",
            text=f"QQ {qq_id} 需要扫码登录。",
            status=NoticeTimelineStatus.WARNING,
            dismiss_mode=NoticeDismissMode.SNOOZE,
        )
        self._emit_sections()

    def _on_qr_code_removed(self, qq_id: str) -> None:
        key = f"qr:{qq_id}"
        self._remove_runtime_notice(key)
        self._dismissed_session_keys.discard(key)
        snoozed_items = self._load_snoozed_items()
        if key in snoozed_items:
            snoozed_items.pop(key, None)
            self._save_snoozed_items(snoozed_items)
        self._emit_sections()

    def _append_runtime_notice(
        self,
        key: str,
        text: str,
        status: NoticeTimelineStatus,
        dismiss_mode: NoticeDismissMode,
    ) -> None:
        self._runtime_notices = [notice for notice in self._runtime_notices if notice.key != key]
        self._runtime_notices.insert(0, _RuntimeNotice(key=key, text=text, status=status, dismiss_mode=dismiss_mode))
        del self._runtime_notices[self._MAX_RUNTIME_NOTICES :]

    def _remove_runtime_notice(self, key: str) -> None:
        self._runtime_notices = [notice for notice in self._runtime_notices if notice.key != key]

    def _emit_sections(self) -> None:
        if self._debug_sections:
            self.sectionsChanged.emit(self._debug_sections)
            return

        self.sectionsChanged.emit(self._build_sections())

    def _build_sections(self) -> list[NoticeTimelineSectionData]:
        sections: list[NoticeTimelineSectionData] = []

        reminder_items = self._filter_items(self._build_reminder_items())
        if reminder_items:
            sections.append(
                NoticeTimelineSectionData(
                    title="提醒",
                    status=self._section_status(reminder_items),
                    items=reminder_items,
                )
            )

        update_items = self._filter_items(self._build_update_items())
        if update_items:
            sections.append(
                NoticeTimelineSectionData(
                    title="更新",
                    status=self._section_status(update_items),
                    items=update_items,
                )
            )

        announcement_items = self._filter_items(self._build_announcement_items())
        if announcement_items:
            sections.append(
                NoticeTimelineSectionData(
                    title="公告",
                    status=self._section_status(announcement_items),
                    items=announcement_items,
                )
            )

        if sections:
            return sections

        return [
            NoticeTimelineSectionData(
                title="提醒",
                status=NoticeTimelineStatus.SUCCESS,
                items=[
                    NoticeTimelineItemData(
                        key="empty:no-notice",
                        text="当前没有需要处理的提醒。",
                        status=NoticeTimelineStatus.SUCCESS,
                    )
                ],
            )
        ]

    def _show_debug_sections(self) -> None:
        self._debug_sections = [
            NoticeTimelineSectionData(
                title="提醒",
                status=NoticeTimelineStatus.WARNING,
                items=[
                    NoticeTimelineItemData(
                        key="debug:offline",
                        text="开发者模式: 测试 Bot 当前离线。",
                        status=NoticeTimelineStatus.WARNING,
                        dismiss_mode=NoticeDismissMode.SESSION,
                    ),
                    NoticeTimelineItemData(
                        key="debug:qr",
                        text="开发者模式: QQ 114514 需要扫码登录。",
                        status=NoticeTimelineStatus.WARNING,
                        dismiss_mode=NoticeDismissMode.SNOOZE,
                    ),
                ],
            ),
            NoticeTimelineSectionData(
                title="更新",
                status=NoticeTimelineStatus.INFO,
                items=[
                    NoticeTimelineItemData(
                        key="debug:update:napcat:v9.9.99",
                        text="NapCat 可更新到 v9.9.99，当前版本为 v1.0.0。",
                        status=NoticeTimelineStatus.INFO,
                        dismiss_mode=NoticeDismissMode.PERSISTENT,
                    ),
                    NoticeTimelineItemData(
                        key="debug:update:desktop:v9.9.99",
                        text="Desktop 可更新到 v9.9.99，当前版本为 v1.0.0。",
                        status=NoticeTimelineStatus.INFO,
                        dismiss_mode=NoticeDismissMode.PERSISTENT,
                    ),
                ],
            ),
            NoticeTimelineSectionData(
                title="公告",
                status=NoticeTimelineStatus.SUCCESS,
                items=[
                    NoticeTimelineItemData(
                        key="debug:announcement:napcat:v9.9.99",
                        text="NapCat v9.9.99: 修复多开登录与通知展示问题。",
                        status=NoticeTimelineStatus.SUCCESS,
                        dismiss_mode=NoticeDismissMode.PERSISTENT,
                    ),
                ],
            ),
        ]
        self._emit_sections()

    def _clear_debug_sections(self) -> None:
        self._runtime_notices = [notice for notice in self._runtime_notices if not notice.text.startswith("开发者模式:")]
        self._debug_sections = []
        self._emit_sections()

    def _dismiss_debug_item(self, key: str) -> bool:
        if not self._debug_sections:
            return False

        changed = False
        next_sections: list[NoticeTimelineSectionData] = []
        for section in self._debug_sections:
            next_items = [item for item in section.items if item.key != key]
            if len(next_items) != len(section.items):
                changed = True
            if next_items:
                next_sections.append(
                    NoticeTimelineSectionData(title=section.title, status=section.status, items=next_items)
                )

        if changed:
            self._debug_sections = next_sections
        return changed

    def _build_reminder_items(self) -> list[NoticeTimelineItemData]:
        items = [
            NoticeTimelineItemData(
                key=notice.key,
                text=notice.text,
                status=notice.status,
                dismiss_mode=notice.dismiss_mode,
            )
            for notice in self._runtime_notices
        ]

        if not self._local_versions.napcat_version:
            items.append(
                NoticeTimelineItemData(
                    key="reminder:install:napcat-missing",
                    text="未检测到 NapCat 安装，暂时无法添加或启动 Bot。",
                    status=NoticeTimelineStatus.WARNING,
                    dismiss_mode=NoticeDismissMode.SNOOZE,
                )
            )

        if not self._local_versions.qq_version:
            items.append(
                NoticeTimelineItemData(
                    key="reminder:install:qq-missing",
                    text="未检测到 QQ 安装，当前无法启动 NapCat 进程。",
                    status=NoticeTimelineStatus.ERROR,
                    dismiss_mode=NoticeDismissMode.SNOOZE,
                )
            )

        if not read_config():
            items.append(
                NoticeTimelineItemData(
                    key="reminder:bot:none",
                    text="还没有添加 Bot，可前往 BOT 页面创建。",
                    status=NoticeTimelineStatus.INFO,
                    dismiss_mode=NoticeDismissMode.SNOOZE,
                )
            )

        if self._is_email_notice_incomplete():
            items.append(
                NoticeTimelineItemData(
                    key="reminder:config:email-incomplete",
                    text="已启用离线邮件通知，但邮箱配置不完整。",
                    status=NoticeTimelineStatus.WARNING,
                    dismiss_mode=NoticeDismissMode.SNOOZE,
                )
            )

        if self._is_webhook_notice_incomplete():
            items.append(
                NoticeTimelineItemData(
                    key="reminder:config:webhook-incomplete",
                    text="已启用离线 WebHook 通知，但配置内容无效。",
                    status=NoticeTimelineStatus.WARNING,
                    dismiss_mode=NoticeDismissMode.SNOOZE,
                )
            )

        return self._deduplicate_items(items)

    def _build_update_items(self) -> list[NoticeTimelineItemData]:
        remote = self._remote_versions
        if remote is None:
            return []

        items: list[NoticeTimelineItemData] = []
        items.extend(
            self._collect_version_update_items(
                name="NapCat",
                slug="napcat",
                local_version=self._local_versions.napcat_version,
                remote_version=remote.napcat_version,
            )
        )
        items.extend(
            self._collect_version_update_items(
                name="QQ",
                slug="qq",
                local_version=self._local_versions.qq_version,
                remote_version=remote.qq_version,
            )
        )
        items.extend(
            self._collect_version_update_items(
                name="Desktop",
                slug="desktop",
                local_version=self._local_versions.ncd_version,
                remote_version=remote.ncd_version,
            )
        )
        return items

    def _build_announcement_items(self) -> list[NoticeTimelineItemData]:
        remote = self._remote_versions
        if remote is None:
            return []

        items: list[NoticeTimelineItemData] = []
        if remote.napcat_version and remote.napcat_update_log and remote.napcat_version != self._local_versions.napcat_version:
            items.append(
                NoticeTimelineItemData(
                    key=f"announcement:napcat:{remote.napcat_version}",
                    text=f"NapCat {remote.napcat_version}: {self._summarize_release_notes(remote.napcat_update_log)}",
                    status=NoticeTimelineStatus.SUCCESS,
                    dismiss_mode=NoticeDismissMode.PERSISTENT,
                )
            )

        if remote.ncd_version and remote.ncd_update_log and remote.ncd_version != self._local_versions.ncd_version:
            items.append(
                NoticeTimelineItemData(
                    key=f"announcement:desktop:{remote.ncd_version}",
                    text=f"Desktop {remote.ncd_version}: {self._summarize_release_notes(remote.ncd_update_log)}",
                    status=NoticeTimelineStatus.SUCCESS,
                    dismiss_mode=NoticeDismissMode.PERSISTENT,
                )
            )

        return items

    def _filter_items(self, items: list[NoticeTimelineItemData]) -> list[NoticeTimelineItemData]:
        ignored_keys = self._load_ignored_keys()
        snoozed_items = self._load_snoozed_items()
        now = int(time())
        return [
            item
            for item in items
            if item.key not in self._dismissed_session_keys
            and item.key not in ignored_keys
            and snoozed_items.get(item.key, 0) <= now
        ]

    def _cleanup_expired_snoozes(self) -> None:
        snoozed_items = self._load_snoozed_items()
        now = int(time())
        active_items = {key: timestamp for key, timestamp in snoozed_items.items() if timestamp > now}
        if active_items != snoozed_items:
            self._save_snoozed_items(active_items)

    @staticmethod
    def _collect_version_update_items(
        name: str,
        slug: str,
        local_version: str | None,
        remote_version: str | None,
    ) -> list[NoticeTimelineItemData]:
        if remote_version is None:
            if local_version is not None:
                return [
                    NoticeTimelineItemData(
                        key=f"update:{slug}:remote-unavailable",
                        text=f"暂时无法获取 {name} 的远程版本信息。",
                        status=NoticeTimelineStatus.WARNING,
                        dismiss_mode=NoticeDismissMode.SESSION,
                    )
                ]
            return []

        if local_version is None:
            return [
                NoticeTimelineItemData(
                    key=f"update:{slug}:install:{remote_version}",
                    text=f"{name} 当前未安装，可安装版本为 {remote_version}。",
                    status=NoticeTimelineStatus.INFO,
                    dismiss_mode=NoticeDismissMode.PERSISTENT,
                )
            ]

        if local_version != remote_version:
            return [
                NoticeTimelineItemData(
                    key=f"update:{slug}:{local_version}->{remote_version}",
                    text=f"{name} 可更新到 {remote_version}，当前版本为 {local_version}。",
                    status=NoticeTimelineStatus.INFO,
                    dismiss_mode=NoticeDismissMode.PERSISTENT,
                )
            ]

        return []

    @staticmethod
    def _section_status(items: list[NoticeTimelineItemData]) -> NoticeTimelineStatus:
        priorities = {
            NoticeTimelineStatus.ERROR: 3,
            NoticeTimelineStatus.WARNING: 2,
            NoticeTimelineStatus.INFO: 1,
            NoticeTimelineStatus.SUCCESS: 0,
        }
        return max(items, key=lambda item: priorities[item.status]).status

    @staticmethod
    def _status_from_level(level: str) -> NoticeTimelineStatus:
        mapping = {
            "success": NoticeTimelineStatus.SUCCESS,
            "info": NoticeTimelineStatus.INFO,
            "warning": NoticeTimelineStatus.WARNING,
            "error": NoticeTimelineStatus.ERROR,
        }
        return mapping.get(level, NoticeTimelineStatus.INFO)

    @staticmethod
    def _summarize_release_notes(text: str) -> str:
        lines = [re.sub(r"^[#\\-*\\s>]+", "", line).strip() for line in text.splitlines()]
        for line in lines:
            if line:
                return line[:88]
        return "发布了新的版本。"

    @staticmethod
    def _deduplicate_items(items: list[NoticeTimelineItemData]) -> list[NoticeTimelineItemData]:
        unique_items: list[NoticeTimelineItemData] = []
        seen: set[str] = set()
        for item in items:
            if item.key in seen:
                continue
            seen.add(item.key)
            unique_items.append(item)
        return unique_items

    @staticmethod
    def _read_json_config(item, default):
        raw_value = cfg.get(item)
        try:
            return json.loads(raw_value)
        except (TypeError, ValueError, json.JSONDecodeError):
            return default

    def _load_ignored_keys(self) -> set[str]:
        data = self._read_json_config(cfg.home_notice_ignored_keys, [])
        return {str(key) for key in data if isinstance(key, str)}

    def _save_ignored_keys(self, keys: set[str]) -> None:
        cfg.set(cfg.home_notice_ignored_keys, json.dumps(sorted(keys), ensure_ascii=False))

    def _load_snoozed_items(self) -> dict[str, int]:
        data = self._read_json_config(cfg.home_notice_snoozed_items, {})
        if not isinstance(data, dict):
            return {}
        return {str(key): int(value) for key, value in data.items() if isinstance(value, (int, float))}

    def _save_snoozed_items(self, items: dict[str, int]) -> None:
        cfg.set(cfg.home_notice_snoozed_items, json.dumps(items, ensure_ascii=False))

    @staticmethod
    def _is_email_notice_incomplete() -> bool:
        if not cfg.get(cfg.bot_offline_email_notice):
            return False

        required_values = (
            cfg.get(cfg.email_receiver),
            cfg.get(cfg.email_sender),
            cfg.get(cfg.email_token),
            cfg.get(cfg.email_stmp_server),
        )
        return any(not value for value in required_values)

    @staticmethod
    def _is_webhook_notice_incomplete() -> bool:
        if not cfg.get(cfg.bot_offline_web_hook_notice):
            return False

        webhook_url = cfg.get(cfg.web_hook_url)
        webhook_json = cfg.get(cfg.web_hook_json)
        if not webhook_url or not webhook_json:
            return True

        try:
            json.loads(webhook_json)
        except (TypeError, ValueError, json.JSONDecodeError):
            return True

        return False


class HomeNoticeDebugCenter(QObject):
    """开发者模式下的首页通知注入通道。"""

    sampleRequested = Signal()
    clearRequested = Signal()


home_notice_debug_center = HomeNoticeDebugCenter()
