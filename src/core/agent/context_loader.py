# -*- coding: utf-8 -*-
"""用户自定义上下文加载模块.

加载用户可编辑的 agent_context.md 文件, 支持基于 mtime 的变更检测和内容缓存. 
当文件超过 MAX_CONTEXT_LENGTH 字符时自动截断并记录警告. 
文件不存在时静默返回空字符串. 
"""

from __future__ import annotations

import logging
from pathlib import Path

logger = logging.getLogger(__name__)

#: 用户上下文文件最大允许字符数
MAX_CONTEXT_LENGTH: int = 32768


class ContextLoader:
    """用户自定义上下文文件加载器.

    通过文件修改时间 (mtime) 检测变更, 缓存文件内容以避免重复读取. 
    超过 MAX_CONTEXT_LENGTH 字符时截断并记录警告. 
    文件不存在时静默返回空字符串, 不记录警告. 

    Args:
        context_file_path: 用户上下文文件的路径. 
    """

    def __init__(self, context_file_path: Path) -> None:
        self._path = context_file_path
        self._cached_content: str = ""
        self._cached_mtime: float | None = None

    def load(self) -> str:
        """加载用户上下文文件内容.

        使用 mtime 缓存策略: 
        - 文件不存在时返回空字符串 (不记录警告) 
        - 文件 mtime 未变化时返回缓存内容
        - 文件 mtime 变化时重新读取并更新缓存
        - 内容超过 MAX_CONTEXT_LENGTH 字符时截断并记录警告

        Returns:
            文件内容字符串, 可能被截断. 文件不存在时返回空字符串. 
        """
        # 文件不存在时静默返回空字符串
        if not self._path.exists():
            self._cached_content = ""
            self._cached_mtime = None
            return ""

        # 获取当前 mtime
        try:
            current_mtime = self._path.stat().st_mtime
        except OSError:
            # 文件在 exists() 检查后被删除等竞态情况
            self._cached_content = ""
            self._cached_mtime = None
            return ""

        # mtime 未变化时返回缓存
        if self._cached_mtime is not None and current_mtime == self._cached_mtime:
            return self._cached_content

        # 读取文件内容
        try:
            content = self._path.read_text(encoding="utf-8")
        except OSError:
            # 读取失败时静默返回空字符串
            self._cached_content = ""
            self._cached_mtime = None
            return ""

        # 超过最大长度时截断并记录警告
        if len(content) > MAX_CONTEXT_LENGTH:
            logger.warning(
                "用户上下文文件 '%s' 超过 %d 字符限制（实际 %d 字符），内容已截断。",
                self._path,
                MAX_CONTEXT_LENGTH,
                len(content),
            )
            content = content[:MAX_CONTEXT_LENGTH]

        # 更新缓存
        self._cached_content = content
        self._cached_mtime = current_mtime

        return content
