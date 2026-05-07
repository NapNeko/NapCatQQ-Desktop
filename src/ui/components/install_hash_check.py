# -*- coding: utf-8 -*-
"""[`run_napcat_archive_hash_check`](src/ui/components/install_hash_check.py):
NapCat 下载完成 -> 解压前的 SHA512 完整性校验 + UI 二次确认 (P5 F1.3).

数据层 [`verify_napcat_archive`](src/core/installation/installers.py) 仅做纯计算与
异常抛出, UI 层(GuideWindow / ComponentPage) 调用本助手把结果映射到
``error_bar`` / ``AskBox`` 流程.
"""
from __future__ import annotations

# 标准库导入
from pathlib import Path
from typing import TYPE_CHECKING

# 项目内模块导入
from src.core.installation.errors import NapCatHashMismatchError
from src.core.installation.installers import verify_napcat_archive
from src.core.logging import LogSource, logger
from src.core.remote.friendly_errors import to_friendly
from src.core.versioning import ReleaseHashService
from src.ui.components.info_bar import error_bar, info_bar
from src.ui.components.message_box import AskBox

if TYPE_CHECKING:
    from PySide6.QtWidgets import QWidget


def run_napcat_archive_hash_check(
    *,
    version: str | None,
    archive_path: Path,
    parent: "QWidget",
    hash_service: ReleaseHashService | None = None,
) -> bool:
    """同步执行 NapCat archive 的 SHA512 校验, 返回是否可继续安装.

    返回值语义:
    - ``True``: 校验通过 / 用户在"无 hash 数据"提示中选择继续 -> 调用方继续解压
    - ``False``: 校验失败 (mismatch) / 用户拒绝继续 -> 调用方应中止安装流程,
      并通过自身 ``handle_operation_failed`` 之类的方法收尾 UI 状态. archive 已被
      数据层删除 (mismatch 路径) 或保留 (用户取消路径).

    Args:
        version: 期望版本字符串 (``v4.18.1`` / ``4.18.1`` 均可). 为 ``None`` 或空串时,
            视为"无版本信息", 走"无 hash 数据"的二次确认分支.
        archive_path: 已经下载到磁盘的 ``NapCat.Shell.zip`` 路径
        parent: AskBox / error_bar 用的父级 widget (主窗口或引导窗口)
        hash_service: 自定义服务实例 (主要用于测试); 缺省走默认 ``ReleaseHashService``
    """
    if not version:
        logger.warning(
            f"NapCat 完整性校验跳过: 未知远程版本号, archive={archive_path}",
            log_source=LogSource.UI,
        )
        return _ask_proceed_without_hash(parent, reason_text=(
            "未能获取 NapCat 远程版本号, 因此无法在上游校验数据中查询期望的 SHA512."
        ))

    service = hash_service if hash_service is not None else ReleaseHashService()
    fetch_result = service.fetch()
    logger.info(
        (
            "NapCat 完整性校验前已尝试拉取上游 hash: "
            f"outcome={fetch_result.outcome.value}, loaded_entries={fetch_result.loaded_entries}"
        ),
        log_source=LogSource.UI,
    )

    try:
        verified = verify_napcat_archive(
            version=version,
            archive_path=archive_path,
            hash_service=service,
        )
    except FileNotFoundError as exc:
        # archive 还没下载就被删了, 极少见; 直接报错让上层重新走一次
        error_bar(f"安装包不存在: {exc}", parent=parent)
        return False
    except NapCatHashMismatchError as exc:
        error_bar(to_friendly(exc), parent=parent)
        return False

    if verified:
        return True

    # ``verified=False``: 上游没有该版本的 hash; 弹二次确认
    return _ask_proceed_without_hash(
        parent,
        reason_text=(
            "上游 NapCat 校验数据中没有该版本的 SHA512, 可能是网络异常 "
            "(上游 release.json 拉取失败且无本地缓存) 或该版本太新尚未发布 hash."
        ),
    )


def _ask_proceed_without_hash(parent: "QWidget", *, reason_text: str) -> bool:
    """弹一个 AskBox 让用户决定是否在缺乏 hash 校验的前提下继续安装."""
    box = AskBox(
        "无法验证安装包完整性",
        (
            f"{reason_text}\n\n"
            "是否在不校验完整性的前提下继续安装?\n"
            "如果你的网络环境不可靠 (例如运营商劫持 / 公共 WiFi / CDN 被投毒), "
            "强烈建议点取消并稍后重试."
        ),
        parent,
    )
    box.yesButton.setText("继续安装")
    box.cancelButton.setText("取消")
    if box.exec():
        info_bar("已在未校验完整性的情况下继续安装", parent=parent)
        return True
    return False


__all__: tuple[str, ...] = ("run_napcat_archive_hash_check",)
